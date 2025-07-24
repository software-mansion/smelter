use std::sync::Arc;

use compositor_render::{Frame, InputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    error::DecoderInitError,
    pipeline::{
        decoder::{
            ffmpeg_h264::FfmpegH264Decoder,
            h264_utils::{AnnexBChunkStream, H264AVCDecoderConfig},
            vulkan_h264::VulkanH264Decoder,
            DecoderThreadHandle, VideoDecoderStream,
        },
        PipelineCtx,
    },
    queue::PipelineEvent,
};

use super::VideoDecoder;

pub fn spawn_video_decoder_thread<Decoder: VideoDecoder, const BUFFER_SIZE: usize>(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    h264_config: Option<H264AVCDecoderConfig>,
    frame_sender: Sender<PipelineEvent<Frame>>,
) -> Result<DecoderThreadHandle, DecoderInitError> {
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

            let result = init_decoder_stream::<Decoder, BUFFER_SIZE>(ctx, h264_config);
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

fn init_decoder_stream<Decoder: VideoDecoder, const BUFFER_SIZE: usize>(
    ctx: Arc<PipelineCtx>,
    h264_config: Option<H264AVCDecoderConfig>,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<Frame>>,
        DecoderThreadHandle,
    ),
    DecoderInitError,
> {
    let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(BUFFER_SIZE);
    let chunk_stream: Box<dyn Iterator<Item = _>> = match Decoder::LABEL {
        FfmpegH264Decoder::LABEL | VulkanH264Decoder::LABEL => {
            let chunk_stream = chunk_receiver.into_iter();
            let chunk_stream = AnnexBChunkStream::new(h264_config, chunk_stream);
            Box::new(chunk_stream)
        }
        _ => Box::new(chunk_receiver.into_iter()),
    };
    let decoded_stream = VideoDecoderStream::<Decoder, _>::new(ctx, chunk_stream)?.flatten();

    Ok((decoded_stream, DecoderThreadHandle { chunk_sender }))
}
