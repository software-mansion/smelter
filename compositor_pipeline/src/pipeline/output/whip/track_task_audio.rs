use std::sync::Arc;

use compositor_render::{error::ErrorStack, OutputId};
use crossbeam_channel::Sender;
use tokio::sync::mpsc;
use tracing::{debug, span, warn, Level};

use crate::{
    audio_mixer::OutputSamples,
    error::EncoderInitError,
    pipeline::{
        encoder::{AudioEncoder, AudioEncoderOptionsExt, AudioEncoderStream, ResampledStream},
        output::rtp::payloader::{PayloaderOptions, PayloaderStream},
        PipelineCtx,
    },
    queue::PipelineEvent,
};

#[derive(Debug)]
pub(crate) struct WhipAudioTrackThreadHandle {
    pub sample_batch_sender: Sender<PipelineEvent<OutputSamples>>,
}

pub fn spawn_audio_track_thread<Encoder: AudioEncoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
    chunks_sender: mpsc::Sender<PipelineEvent<rtp::packet::Packet>>,
) -> Result<WhipAudioTrackThreadHandle, EncoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("RTP audio track thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "WHIP: audio encoder + payloader thread",
                output_id = output_id.to_string(),
                encoder = Encoder::LABEL
            )
            .entered();

            let result = init_stream::<Encoder>(ctx, encoder_options, payloader_options);
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
                if chunks_sender.blocking_send(event).is_err() {
                    warn!("Failed to send encoded audio chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Encoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_stream<Encoder: AudioEncoder>(
    ctx: Arc<PipelineCtx>,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<rtp::packet::Packet>>,
        WhipAudioTrackThreadHandle,
    ),
    EncoderInitError,
> {
    let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);

    let resampled_stream = ResampledStream::new(
        sample_batch_receiver.into_iter(),
        ctx.mixing_sample_rate,
        encoder_options.sample_rate(),
    )?
    .flatten();

    let (encoded_stream, _config) =
        AudioEncoderStream::<Encoder, _>::new(ctx, encoder_options, resampled_stream)?;

    let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

    let stream = payloaded_stream.flatten().filter_map(|event| match event {
        Ok(event) => Some(event),
        Err(err) => {
            warn!(
                "Depayloading error: {}",
                ErrorStack::new(&err).into_string()
            );
            None
        }
    });

    Ok((
        stream,
        WhipAudioTrackThreadHandle {
            sample_batch_sender,
        },
    ))
}
