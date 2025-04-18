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
mod ffmpeg_vp9;
#[cfg(feature = "vk-video")]
mod vulkan_video;

pub fn start_video_decoder_thread(
    options: VideoDecoderOptions,
    pipeline_ctx: &PipelineCtx,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    input_id: InputId,
    send_eos: bool,
) -> Result<(), InputInitError> {
    match options.decoder {
        VideoDecoder::FFmpegH264 => ffmpeg_h264::start_ffmpeg_decoder_thread(
            pipeline_ctx,
            chunks_receiver,
            frame_sender,
            input_id,
            send_eos,
        ),

        VideoDecoder::FFmpegVp8 => ffmpeg_vp8::start_ffmpeg_decoder_thread(
            pipeline_ctx,
            chunks_receiver,
            frame_sender,
            input_id,
            send_eos,
        ),

        VideoDecoder::FFmpegVp9 => ffmpeg_vp9::start_ffmpeg_decoder_thread(
            pipeline_ctx,
            chunks_receiver,
            frame_sender,
            input_id,
            send_eos,
        ),

        #[cfg(feature = "vk-video")]
        VideoDecoder::VulkanVideoH264 => vulkan_video::start_vulkan_video_decoder_thread(
            pipeline_ctx,
            chunks_receiver,
            frame_sender,
            input_id,
            send_eos,
        ),
    }
}
