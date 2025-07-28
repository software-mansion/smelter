use std::sync::Arc;

use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::pipeline::{
    decoder::{AudioDecoderStream, DecoderThreadHandle},
    resampler::decoder_resampler::ResampledDecoderStream,
};
use crate::prelude::*;

use super::AudioDecoder;

pub fn spawn_audio_decoder_thread<Decoder: AudioDecoder, const BUFFER_SIZE: usize>(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    options: Decoder::Options,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
) -> Result<DecoderThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Decoder thread for input {}", &input_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Audio decoder thread",
                input_id = input_id.to_string(),
                decoder = Decoder::LABEL
            )
            .entered();

            let result = init_decoder_stream::<Decoder, BUFFER_SIZE>(ctx, options);
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
                if samples_sender.send(event).is_err() {
                    warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_decoder_stream<Decoder: AudioDecoder, const BUFFER_SIZE: usize>(
    ctx: Arc<PipelineCtx>,
    options: Decoder::Options,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<InputAudioSamples>>,
        DecoderThreadHandle,
    ),
    DecoderInitError,
> {
    let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(BUFFER_SIZE);
    let output_sample_rate = ctx.mixing_sample_rate;

    let decoded_stream =
        AudioDecoderStream::<Decoder, _>::new(ctx, options, chunk_receiver.into_iter())?;

    let resampled_stream =
        ResampledDecoderStream::new(output_sample_rate, decoded_stream.flatten());

    Ok((
        resampled_stream.flatten(),
        DecoderThreadHandle { chunk_sender },
    ))
}
