use std::sync::Arc;

use compositor_render::OutputId;
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    audio_mixer::OutputSamples,
    error::DecoderInitError,
    pipeline::{DecoderOutputEvent, PipelineCtx},
    queue::PipelineEvent,
};

use super::{
    AudioDecoder, AudioDecoderConfig, AudioDecoderOptionsExt, AudioDecoderStream, ResampledStream,
};

pub(crate) struct AudioDecoderThreadHandle {
    pub sample_batch_sender: Sender<PipelineEvent<OutputSamples>>,
    pub config: AudioDecoderConfig,
}

pub fn spawn_audio_encoder_thread<Decoder: AudioDecoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    options: Decoder::Options,
    chunks_sender: Sender<DecoderOutputEvent>,
) -> Result<AudioDecoderThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Decoder thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Decoder thread",
                output_id = output_id.to_string(),
                encoder = Decoder::LABEL
            )
            .entered();

            let result = init_encoder_stream::<Decoder>(ctx, options);
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

fn init_encoder_stream<Decoder: AudioDecoder>(
    ctx: Arc<PipelineCtx>,
    options: Decoder::Options,
) -> Result<
    (
        impl Iterator<Item = DecoderOutputEvent>,
        AudioDecoderThreadHandle,
    ),
    DecoderInitError,
> {
    let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);
    let resampled_stream = ResampledStream::new(
        sample_batch_receiver.into_iter(),
        ctx.mixing_sample_rate,
        options.sample_rate(),
    )?
    .flatten();

    let (encoded_stream, config) =
        AudioDecoderStream::<Decoder, _>::new(ctx, options, resampled_stream)?;

    let stream = encoded_stream.flatten().map(|event| match event {
        PipelineEvent::Data(chunk) => DecoderOutputEvent::Data(chunk),
        PipelineEvent::EOS => DecoderOutputEvent::AudioEOS,
    });
    Ok((
        stream,
        AudioDecoderThreadHandle {
            sample_batch_sender,
            config,
        },
    ))
}

