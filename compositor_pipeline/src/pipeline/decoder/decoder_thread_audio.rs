use std::sync::Arc;

use compositor_render::OutputId;
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    audio_mixer::InputSamples,
    error::DecoderInitError,
    pipeline::{
        decoder::AudioDecoderStream, resampler::decoder_resampler::ResampledDecoderStream,
        types::DecodedSamples, EncodedChunk, PipelineCtx,
    },
    queue::PipelineEvent,
};

use super::AudioDecoder;

pub(crate) struct AudioDecoderThreadHandle {
    pub chunk_sender: Sender<PipelineEvent<EncodedChunk>>,
}

pub fn spawn_audio_encoder_thread<Decoder: AudioDecoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    options: Decoder::Options,
    chunks_sender: Sender<PipelineEvent<DecodedSamples>>,
) -> Result<AudioDecoderThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Decoder thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Audio decoder thread",
                output_id = output_id.to_string(),
                encoder = Decoder::LABEL
            )
            .entered();

            let result = init_decoder_stream::<Decoder>(ctx, options);
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
                if chunks_sender.send(event).is_err() {
                    warn!("Failed to send encoded audio chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_decoder_stream<Decoder: AudioDecoder>(
    ctx: Arc<PipelineCtx>,
    options: Decoder::Options,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<InputSamples>>,
        AudioDecoderThreadHandle,
    ),
    DecoderInitError,
> {
    let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(5);
    let output_sample_rate = ctx.mixing_sample_rate;

    let decoded_stream =
        AudioDecoderStream::<Decoder, _>::new(ctx, options, chunk_receiver.into_iter())?.flatten();

    let resampled_stream =
        ResampledDecoderStream::new(decoded_stream, output_sample_rate).flatten();

    Ok((resampled_stream, AudioDecoderThreadHandle { chunk_sender }))
}
