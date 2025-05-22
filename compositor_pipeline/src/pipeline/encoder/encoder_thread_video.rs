use std::sync::Arc;

use compositor_render::{Frame, OutputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    error::EncoderInitError,
    pipeline::{EncoderOutputEvent, PipelineCtx},
    queue::PipelineEvent,
};

use super::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream};

pub(crate) struct VideoEncoderThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub fn spawn_video_encoder_thread<Encoder: VideoEncoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    options: Encoder::Options,
    chunks_sender: Sender<EncoderOutputEvent>,
) -> Result<VideoEncoderThreadHandle, EncoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Encoder thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Encoder thread",
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
                    warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Encoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_encoder_stream<Encoder: VideoEncoder>(
    ctx: Arc<PipelineCtx>,
    options: Encoder::Options,
) -> Result<
    (
        impl Iterator<Item = EncoderOutputEvent>,
        VideoEncoderThreadHandle,
    ),
    EncoderInitError,
> {
    let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
    let (encoded_stream, encoder_ctx) =
        VideoEncoderStream::<Encoder, _>::new(ctx, options, frame_receiver.into_iter())?;

    let stream = encoded_stream.flatten().map(|event| match event {
        PipelineEvent::Data(chunk) => EncoderOutputEvent::Data(chunk),
        PipelineEvent::EOS => EncoderOutputEvent::VideoEOS,
    });
    Ok((
        stream,
        VideoEncoderThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
            config: encoder_ctx.config,
        },
    ))
}

impl VideoEncoderThreadHandle {
    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
