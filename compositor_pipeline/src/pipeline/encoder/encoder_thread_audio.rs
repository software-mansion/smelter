use std::sync::Arc;

use compositor_render::OutputId;
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::pipeline::{
    encoder::{AudioEncoder, AudioEncoderConfig, AudioEncoderStream},
    resampler::encoder_resampler::ResampledForEncoderStream,
};
use crate::prelude::*;

pub(crate) struct AudioEncoderThreadHandle {
    pub sample_batch_sender: Sender<PipelineEvent<OutputAudioSamples>>,
    pub config: AudioEncoderConfig,
}

pub fn spawn_audio_encoder_thread<Encoder: AudioEncoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    options: Encoder::Options,
    chunks_sender: Sender<EncodedOutputEvent>,
) -> Result<AudioEncoderThreadHandle, EncoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Video encoder thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Video encoder thread",
                output_id = output_id.to_string(),
                encoder = Encoder::LABEL
            )
            .entered();

            let result = init_encoder_stream::<Encoder>(ctx, options);
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
            debug!("Encoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_encoder_stream<Encoder: AudioEncoder>(
    ctx: Arc<PipelineCtx>,
    options: Encoder::Options,
) -> Result<
    (
        impl Iterator<Item = EncodedOutputEvent>,
        AudioEncoderThreadHandle,
    ),
    EncoderInitError,
> {
    let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);
    let resampled_stream = ResampledForEncoderStream::new(
        sample_batch_receiver.into_iter(),
        ctx.mixing_sample_rate,
        options.sample_rate(),
    )
    .flatten();

    let (encoded_stream, config) =
        AudioEncoderStream::<Encoder, _>::new(ctx, options, resampled_stream)?;

    let stream = encoded_stream.flatten().map(|event| match event {
        PipelineEvent::Data(chunk) => EncodedOutputEvent::Data(chunk),
        PipelineEvent::EOS => EncodedOutputEvent::AudioEOS,
    });
    Ok((
        stream,
        AudioEncoderThreadHandle {
            sample_batch_sender,
            config,
        },
    ))
}

impl AudioEncoderThreadHandle {
    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
