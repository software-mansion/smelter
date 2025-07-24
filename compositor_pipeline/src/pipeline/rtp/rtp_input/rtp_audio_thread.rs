use std::sync::Arc;

use compositor_render::InputId;
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    audio_mixer::InputSamples,
    error::DecoderInitError,
    pipeline::{
        decoder::{AudioDecoder, AudioDecoderStream},
        resampler::decoder_resampler::ResampledDecoderStream,
        rtp::{
            depayloader::{DepayloaderOptions, DepayloaderStream},
            RtpPacket,
        },
        PipelineCtx,
    },
    queue::PipelineEvent,
};

pub(crate) struct RtpAudioTrackThreadHandle {
    pub rtp_packet_sender: Sender<PipelineEvent<RtpPacket>>,
    pub sample_rate: u32,
}

pub fn spawn_rtp_audio_thread<Decoder: AudioDecoder>(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    sample_rate: u32,
    decoder_options: Decoder::Options,
    depayloader_options: DepayloaderOptions,
    decoded_samples_sender: Sender<PipelineEvent<InputSamples>>,
) -> Result<RtpAudioTrackThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("RTP audio track thread for input {}", &input_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Decoder thread",
                input_id = input_id.to_string(),
                decoder = Decoder::LABEL
            )
            .entered();

            let result =
                init_stream::<Decoder>(ctx, decoder_options, depayloader_options, sample_rate);
            let stream = match result {
                Ok((stream, handle)) => {
                    result_sender.send(Ok(handle)).unwrap();
                    stream
                }
                Err(err) => {
                    result_sender.send(Err(err)).unwrap();
                    return;
                }
            };
            for event in stream {
                if decoded_samples_sender.send(event).is_err() {
                    warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_stream<Decoder: AudioDecoder>(
    ctx: Arc<PipelineCtx>,
    decoder_options: Decoder::Options,
    depayloader_options: DepayloaderOptions,
    sample_rate: u32,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<InputSamples>>,
        RtpAudioTrackThreadHandle,
    ),
    DecoderInitError,
> {
    let mixing_sample_rate = ctx.mixing_sample_rate;
    let (rtp_packet_sender, rtp_packet_receiver) = crossbeam_channel::bounded(5);

    let depayloader_stream =
        DepayloaderStream::new(depayloader_options, rtp_packet_receiver.into_iter());

    let decoder_stream =
        AudioDecoderStream::<Decoder, _>::new(ctx, decoder_options, depayloader_stream.flatten())?;

    let resampled_stream =
        ResampledDecoderStream::new(mixing_sample_rate, decoder_stream.flatten());

    Ok((
        resampled_stream.flatten(),
        RtpAudioTrackThreadHandle {
            rtp_packet_sender,
            sample_rate,
        },
    ))
}
