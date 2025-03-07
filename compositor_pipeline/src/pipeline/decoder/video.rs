use compositor_render::{Frame, InputId};
use crossbeam_channel::{Receiver, Sender};

use crate::{
    error::InputInitError,
    pipeline::{types::EncodedChunk, PipelineCtx, VideoDecoder},
    queue::PipelineEvent,
};

use super::VideoDecoderOptions;

mod ffmpeg_h264;
mod ffmpeg_vp8;
#[cfg(feature = "vk-video")]
mod vulkan_video;

pub fn start_video_decoder_thread(
    options: VideoDecoderOptions,
    pipeline_ctx: &PipelineCtx,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    input_id: InputId,
) -> Result<(), InputInitError> {
    match options.decoder {
        VideoDecoder::FFmpegH264 => ffmpeg_h264::start_ffmpeg_decoder_thread(
            pipeline_ctx,
            chunks_receiver,
            frame_sender,
            input_id,
        ),

        VideoDecoder::FFmpegVp8 => ffmpeg_vp8::start_ffmpeg_decoder_thread(
            pipeline_ctx,
            chunks_receiver,
            frame_sender,
            input_id,
        ),

        #[cfg(feature = "vk-video")]
        VideoDecoder::VulkanVideoH264 => vulkan_video::start_vulkan_video_decoder_thread(
            pipeline_ctx,
            chunks_receiver,
            frame_sender,
            input_id,
        ),
    }
}
