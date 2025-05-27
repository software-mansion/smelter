use std::time::Duration;

use compositor_pipeline::{
    pipeline::{self, input},
    queue,
};
use tracing::warn;

use crate::*;

impl TryFrom<Mp4Input> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: Mp4Input) -> Result<Self, Self::Error> {
        let Mp4Input {
            url,
            path,
            required,
            offset_ms,
            should_loop,
            video_decoder,
        } = value;

        const BAD_URL_PATH_SPEC: &str =
            "Exactly one of `url` or `path` has to be specified in a register request for an mp4 input.";

        let source = match (url, path) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(TypeError::new(BAD_URL_PATH_SPEC));
            }
            (Some(url), None) => input::mp4::Source::Url(url),
            (None, Some(path)) => input::mp4::Source::File(path.into()),
        };

        let queue_options = queue::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        if video_decoder.is_some() {
            warn!("video_decoder option is deprecated.")
        }

        let video_decoder = match video_decoder.unwrap_or(VideoDecoder::FfmpegH264) {
            VideoDecoder::FfmpegH264 => pipeline::VideoDecoder::FFmpegH264,
            VideoDecoder::FfmpegVp8 => return Err(TypeError::new("MP4 VP8 input not supported")),
            VideoDecoder::FfmpegVp9 => return Err(TypeError::new("MP4 VP9 input not supported")),

            #[cfg(feature = "vk-video")]
            VideoDecoder::VulkanH264 | VideoDecoder::VulkanVideo => {
                pipeline::VideoDecoder::VulkanVideoH264
            }

            #[cfg(not(feature = "vk-video"))]
            VideoDecoder::VulkanH264 | VideoDecoder::VulkanVideo => {
                return Err(TypeError::new(super::NO_VULKAN_VIDEO))
            }
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: input::InputOptions::Mp4(input::mp4::Mp4Options {
                source,
                should_loop: should_loop.unwrap_or(false),
                video_decoder,
            }),
            queue_options,
        })
    }
}
