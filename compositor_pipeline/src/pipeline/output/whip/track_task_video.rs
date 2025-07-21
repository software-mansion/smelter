use std::sync::Arc;

use compositor_render::{error::ErrorStack, Frame, OutputId};
use crossbeam_channel::Sender;
use tokio::sync::mpsc;
use tracing::{debug, span, warn, Level};

use crate::{
    error::EncoderInitError,
    pipeline::{
        encoder::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream},
        rtp::{
            payloader::{PayloaderOptions, PayloaderStream},
            RtpPacket,
        },
        PipelineCtx,
    },
    queue::PipelineEvent,
};

#[derive(Debug)]
pub(crate) struct WhipVideoTrackThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub fn spawn_video_track_thread<Encoder: VideoEncoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
    chunks_sender: mpsc::Sender<RtpPacket>,
) -> Result<WhipVideoTrackThreadHandle, EncoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("WHIP video track thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "WHIP: video encoder + payloader thread",
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
                    warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Encoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_stream<Encoder: VideoEncoder>(
    ctx: Arc<PipelineCtx>,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
) -> Result<(impl Iterator<Item = RtpPacket>, WhipVideoTrackThreadHandle), EncoderInitError> {
    let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
    let (encoded_stream, encoder_ctx) =
        VideoEncoderStream::<Encoder, _>::new(ctx, encoder_options, frame_receiver.into_iter())?;

    let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

    let stream = payloaded_stream.flatten().filter_map(|event| match event {
        Ok(PipelineEvent::Data(packet)) => Some(packet),
        Ok(PipelineEvent::EOS) => None,
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
        WhipVideoTrackThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
            config: encoder_ctx.config,
        },
    ))
}
