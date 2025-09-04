use itertools::Itertools;
use std::time::Duration;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<WhipInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhipInput) -> Result<Self, Self::Error> {
        let WhipInput {
            video,
            required,
            offset_ms,
            bearer_token,
            endpoint_override,
        } = value;

        // TODO: move this logic to pipeline and resolve the final values
        // when we know if vulkan decoder is supported
        let whip_options = match video {
            Some(options) => {
                let video_preferences = match options.decoder_preferences.as_deref() {
                    Some([]) | None => vec![WhipVideoDecoder::Any],
                    Some(v) => v.to_vec(),
                };
                let video_preferences: Vec<pipeline::VideoDecoderOptions> = video_preferences
                    .into_iter()
                    .flat_map(|codec| match codec {
                        WhipVideoDecoder::FfmpegH264 => {
                            vec![pipeline::VideoDecoderOptions::FfmpegH264]
                        }
                        #[cfg(feature = "vk-video")]
                        WhipVideoDecoder::VulkanH264 => {
                            vec![pipeline::VideoDecoderOptions::VulkanH264]
                        }
                        WhipVideoDecoder::FfmpegVp8 => {
                            vec![pipeline::VideoDecoderOptions::FfmpegVp8]
                        }
                        WhipVideoDecoder::FfmpegVp9 => {
                            vec![pipeline::VideoDecoderOptions::FfmpegVp9]
                        }
                        #[cfg(not(feature = "vk-video"))]
                        WhipVideoDecoder::Any => {
                            vec![
                                pipeline::VideoDecoderOptions::FfmpegVp9,
                                pipeline::VideoDecoderOptions::FfmpegVp8,
                                pipeline::VideoDecoderOptions::FfmpegH264,
                            ]
                        }
                        #[cfg(feature = "vk-video")]
                        WhipVideoDecoder::Any => {
                            vec![
                                pipeline::VideoDecoderOptions::FfmpegVp9,
                                pipeline::VideoDecoderOptions::FfmpegVp8,
                                pipeline::VideoDecoderOptions::VulkanH264,
                            ]
                        }
                        #[cfg(not(feature = "vk-video"))]
                        WhipVideoDecoder::VulkanH264 => vec![],
                    })
                    .unique()
                    .collect();
                pipeline::WhipInputOptions {
                    video_preferences,
                    bearer_token,
                    endpoint_override,
                }
            }
            None => pipeline::WhipInputOptions {
                #[cfg(not(feature = "vk-video"))]
                video_preferences: vec![
                    pipeline::VideoDecoderOptions::FfmpegH264,
                    pipeline::VideoDecoderOptions::FfmpegVp8,
                    pipeline::VideoDecoderOptions::FfmpegVp9,
                ],
                #[cfg(feature = "vk-video")]
                video_preferences: vec![
                    pipeline::VideoDecoderOptions::VulkanH264,
                    pipeline::VideoDecoderOptions::FfmpegH264,
                    pipeline::VideoDecoderOptions::FfmpegVp8,
                    pipeline::VideoDecoderOptions::FfmpegVp9,
                ],
                bearer_token,
                endpoint_override,
            },
        };

        let input_options = pipeline::ProtocolInputOptions::Whip(whip_options);

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        Ok(pipeline::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}
