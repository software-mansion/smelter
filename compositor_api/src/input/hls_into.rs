use std::time::Duration;

use compositor_pipeline::{
    pipeline::{
        self, decoder,
        input::{self, hls},
    },
    queue,
};

use crate::*;

impl TryFrom<HlsInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: HlsInput) -> Result<Self, Self::Error> {
        let HlsInput {
            url,
            video_decoder,
            required,
            offset_ms,
        } = value;

        let queue_options = queue::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        let input_options = hls::HlsInputOptions {
            url,
            video_decoder: match video_decoder.unwrap_or(VideoDecoder::FfmpegH264) {
                VideoDecoder::FfmpegH264 => decoder::VideoDecoderOptions {
                    decoder: pipeline::VideoDecoder::FFmpegH264,
                },

                VideoDecoder::FfmpegVp8 => decoder::VideoDecoderOptions {
                    decoder: pipeline::VideoDecoder::FFmpegVp8,
                },

                VideoDecoder::FfmpegVp9 => decoder::VideoDecoderOptions {
                    decoder: pipeline::VideoDecoder::FFmpegVp9,
                },

                #[cfg(feature = "vk-video")]
                VideoDecoder::VulkanH264 => decoder::VideoDecoderOptions {
                    decoder: pipeline::VideoDecoder::VulkanVideoH264,
                },

                #[cfg(feature = "vk-video")]
                VideoDecoder::VulkanVideo => {
                    tracing::warn!("vulkan_video option is deprecated, use vulkan_h264 instead.");
                    decoder::VideoDecoderOptions {
                        decoder: pipeline::VideoDecoder::VulkanVideoH264,
                    }
                }

                #[cfg(not(feature = "vk-video"))]
                VideoDecoder::VulkanH264 | VideoDecoder::VulkanVideo => {
                    return Err(TypeError::new(super::NO_VULKAN_VIDEO))
                }
            },
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: input::InputOptions::Hls(input_options),
            queue_options,
        })
    }
}
