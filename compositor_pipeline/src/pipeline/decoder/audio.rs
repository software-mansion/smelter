use compositor_render::InputId;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::{debug, error};
use tracing::{span, trace, Level};

extern crate opus as lib_opus;
use crate::{
    audio_mixer::InputSamples,
    error::InputInitError,
    pipeline::types::{DecodedSamples, EncodedChunk},
    queue::PipelineEvent,
};

use self::{fdk_aac::AacDecoder, opus::OpusDecoder, resampler::Resampler};

use super::{AudioDecoderOptions, OpusDecoderOptions};
pub use fdk_aac::AacDecoderError;

mod fdk_aac;
mod opus;
mod resampler;

#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
    #[error(transparent)]
    OpusError(#[from] lib_opus::Error),
    #[error(transparent)]
    AacDecoder(#[from] AacDecoderError),
}

trait AudioDecoderExt {
    fn decode(&mut self, encoded_chunk: EncodedChunk)
        -> Result<Vec<DecodedSamples>, DecodingError>;

    fn decoded_sample_rate(&self) -> u32;
}

pub fn start_audio_resampler_only_thread(
    input_sample_rate: u32,
    mixing_sample_rate: u32,
    raw_samples_receiver: Receiver<PipelineEvent<DecodedSamples>>,
    samples_sender: Sender<PipelineEvent<InputSamples>>,
    input_id: InputId,
) -> Result<(), InputInitError> {
    let (decoder_init_result_sender, decoder_init_result_receiver) = bounded(0);
    std::thread::Builder::new()
        .name(format!("Decoder thread for input {}", input_id.clone()))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Audio decoder",
                input_id = input_id.to_string()
            )
            .entered();

            run_resampler_only_thread(
                input_sample_rate,
                mixing_sample_rate,
                raw_samples_receiver,
                samples_sender,
                decoder_init_result_sender,
            );
        })
        .unwrap();

    match decoder_init_result_receiver.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(init_err)) => Err(init_err),
        Err(_recv_err) => Err(InputInitError::CannotReadInitResult),
    }
}

fn run_resampler_only_thread(
    input_sample_rate: u32,
    mixing_sample_rate: u32,
    raw_samples_receiver: Receiver<PipelineEvent<DecodedSamples>>,
    samples_sender: Sender<PipelineEvent<InputSamples>>,
    init_result_sender: Sender<Result<(), InputInitError>>,
) {
    let mut resampler = match Resampler::new(input_sample_rate, mixing_sample_rate) {
        Ok(resampler) => {
            if init_result_sender.send(Ok(())).is_err() {
                error!("Failed to send rescaler init result.");
            }
            resampler
        }
        Err(err) => {
            if init_result_sender.send(Err(err)).is_err() {
                error!("Failed to send rescaler init result.");
            }
            return;
        }
    };

    for event in raw_samples_receiver {
        let PipelineEvent::Data(raw_sample) = event else {
            break;
        };

        for input_samples in resampler.resample(raw_sample) {
            if samples_sender
                .send(PipelineEvent::Data(input_samples))
                .is_err()
            {
                debug!("Failed to send decoded input samples.");
            };
        }
    }

    if samples_sender.send(PipelineEvent::EOS).is_err() {
        debug!("Failed to send EOS message.")
    }
}

pub fn start_audio_decoder_thread(
    opts: AudioDecoderOptions,
    mixing_sample_rate: u32,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    samples_sender: Sender<PipelineEvent<InputSamples>>,
    input_id: InputId,
    send_eos: bool,
) -> Result<(), InputInitError> {
    let (init_result_sender, init_result_receiver) = bounded(0);
    std::thread::Builder::new()
        .name(format!("Decoder thread for input {}", input_id.clone()))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Audio decoder",
                input_id = input_id.to_string()
            )
            .entered();

            // This ensures that EOS is send only once
            let sender = |samples: InputSamples| {
                if samples_sender.send(PipelineEvent::Data(samples)).is_err() {
                    debug!("Failed to send decoded input samples.");
                };
            };

            run_decoding(
                opts,
                mixing_sample_rate,
                chunks_receiver,
                sender,
                init_result_sender,
            );

            if send_eos && samples_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        })
        .unwrap();

    match init_result_receiver.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(init_err)) => Err(init_err),
        Err(_recv_err) => Err(InputInitError::CannotReadInitResult),
    }
}

/// init_result_sender sends:
/// - true init result for Opus
/// - always ok for AAC (aac sample rate is unknown at register time, first chunk is need to determine it)
fn run_decoding<F>(
    opts: AudioDecoderOptions,
    mixing_sample_rate: u32,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    samples_sender: F,
    init_result_sender: Sender<Result<(), InputInitError>>,
) where
    F: Fn(InputSamples),
{
    // AAC decoder output can have any sample rate, therefore resampling it to output sample rate is needed.
    // In Opus decoder we can configure output sampler rate and decoder performs resampling, however if output sample
    // rate is not supported by Opus (e.g. 44100Hz), then resampling is also needed.

    // In case of AAC decoder, decoded samples rate is unknown on registration
    // (decoder has to parse the first received chunk to determine it).
    // This means true AAC init result can't be send back.
    // Registering input can't be blocked on init_result.recv() by this thread.
    // It means that AAC decoder output sample rate is unknown at register
    // and AAC decoder init error and resampler init error won't be send back.

    let send_result = |result: Result<(), InputInitError>| {
        if init_result_sender.send(result).is_err() {
            error!("Failed to send decoder init result.");
        }
    };

    match opts {
        AudioDecoderOptions::Opus(opus_decoder_opts) => {
            // Opus decoder initialization doesn't require input stream data,
            // so this can wait and send init result
            match init_opus_decoder(opus_decoder_opts, mixing_sample_rate) {
                Ok((mut decoder, mut resampler)) => {
                    send_result(Ok(()));
                    run_decoding_loop(
                        chunks_receiver,
                        &mut decoder,
                        &mut resampler,
                        samples_sender,
                    )
                }
                Err(err) => {
                    send_result(Err(err));
                }
            }
        }
        AudioDecoderOptions::Aac(aac_decoder_opts) => {
            // unfortunately, since this decoder needs to inspect the first data chunk
            // to initialize, we cannot block in the main thread and wait for it to
            // report a success or failure.
            send_result(Ok(()));
            let first_chunk = match chunks_receiver.recv() {
                Ok(PipelineEvent::Data(first_chunk)) => first_chunk,
                Ok(PipelineEvent::EOS) => {
                    return;
                }
                Err(_) => {
                    error!("Failed to read the first chunk from input to initialize decoder.");
                    return;
                }
            };
            let init_res = AacDecoder::new(aac_decoder_opts, first_chunk)
                .map(|decoder| {
                    let resampler =
                        Resampler::new(decoder.decoded_sample_rate(), mixing_sample_rate)?;
                    Ok((decoder, resampler))
                })
                .and_then(|res| res);

            match init_res {
                Ok((mut decoder, mut resampler)) => run_decoding_loop(
                    chunks_receiver,
                    &mut decoder,
                    &mut resampler,
                    samples_sender,
                ),
                Err(err) => {
                    error!("Fatal AAC decoder initialization error. {}", err);
                }
            }
        }
    }
}

fn run_decoding_loop<Decoder, F>(
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    decoder: &mut Decoder,
    resampler: &mut Resampler,
    samples_sender: F,
) where
    Decoder: AudioDecoderExt,
    F: Fn(InputSamples),
{
    for event in chunks_receiver {
        let PipelineEvent::Data(encoded_chunk) = event else {
            break;
        };

        let decoded_samples_vec = match decoder.decode(encoded_chunk) {
            Ok(decoded_samples) => decoded_samples,
            Err(err) => {
                error!("Failed to decode samples. Error: {}", err);
                continue;
            }
        };

        trace!(?decoded_samples_vec, "Audio decoder produced samples");
        for decoded_samples in decoded_samples_vec {
            for input_samples in resampler.resample(decoded_samples) {
                samples_sender(input_samples)
            }
        }
    }
}

fn init_opus_decoder(
    opus_decoder_opts: OpusDecoderOptions,
    mixing_sample_rate: u32,
) -> Result<(OpusDecoder, Resampler), InputInitError> {
    let decoder = OpusDecoder::new(opus_decoder_opts, mixing_sample_rate)?;
    let resampler = Resampler::new(decoder.decoded_sample_rate(), mixing_sample_rate)?;
    Ok((decoder, resampler))
}
