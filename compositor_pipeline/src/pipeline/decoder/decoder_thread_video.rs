use std::sync::Arc;

use compositor_render::{Frame, InputId, OutputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    error::DecoderInitError,
    pipeline::{decoder::VideoDecoderStream, EncodedChunk, PipelineCtx},
    queue::PipelineEvent,
};

use super::VideoDecoder;

pub(crate) struct VideoDecoderThreadHandle {
    pub chunk_sender: Sender<PipelineEvent<EncodedChunk>>,
}

pub fn spawn_video_decoder_thread<Decoder: VideoDecoder>(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    frame_sender: Sender<PipelineEvent<Frame>>,
) -> Result<VideoDecoderThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Decoder thread for input {}", &input_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Video decoder thread",
                input_id = input_id.to_string(),
                decoder = Decoder::LABEL
            )
            .entered();

            let result = init_decoder_stream::<Decoder>(ctx);
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
                if frame_sender.send(event).is_err() {
                    warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_decoder_stream<Decoder: VideoDecoder>(
    ctx: Arc<PipelineCtx>,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<Frame>>,
        VideoDecoderThreadHandle,
    ),
    DecoderInitError,
> {
    let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(5);
    let decoded_stream =
        VideoDecoderStream::<Decoder, _>::new(ctx, chunk_receiver.into_iter())?.flatten();

    Ok((decoded_stream, VideoDecoderThreadHandle { chunk_sender }))
}
