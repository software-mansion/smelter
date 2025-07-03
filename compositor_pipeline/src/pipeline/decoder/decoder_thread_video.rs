use std::sync::Arc;

use compositor_render::{Frame, InputId, OutputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{error::DecoderInitError, pipeline::PipelineCtx, queue::PipelineEvent};

use super::VideoDecoder;

pub(crate) struct VideoDecoderThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
}

pub fn spawn_video_decoder_thread<Decoder: VideoDecoder>(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    options: Decoder::Options,
    chunks_sender: Sender<PipelineEvent<Frame>>,
) -> Result<VideoDecoderThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Decoder thread for input {}", &input_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Decoder thread",
                input_id = input_id.to_string(),
                decoder = Decoder::LABEL
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
                    warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_encoder_stream<Decoder: VideoDecoder>(
    ctx: Arc<PipelineCtx>,
    options: Decoder::Options,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<Frame>>,
        VideoDecoderThreadHandle,
    ),
    DecoderInitError,
> {
    let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
    let (encoded_stream, encoder_ctx) =
        VideoDecoderStream::<Decoder, _>::new(ctx, options, frame_receiver.into_iter())?;

    let stream = encoded_stream.flatten().map(|event| match event {
        PipelineEvent::Data(chunk) => DecoderOutputEvent::Data(chunk),
        PipelineEvent::EOS => DecoderOutputEvent::VideoEOS,
    });
    Ok((
        stream,
        VideoDecoderThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
            config: encoder_ctx.config,
        },
    ))
}

impl VideoDecoderThreadHandle {
    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
