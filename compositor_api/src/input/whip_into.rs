use itertools::Itertools;
use std::time::Duration;
use tracing::warn;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<WhipServer> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhipServer) -> Result<Self, Self::Error> {
        let WhipServer {
            video,
            audio: _,
            required,
            offset_ms,
            bearer_token,
            endpoint_override,
        } = value;

        if video.clone().and_then(|v| v.decoder.clone()).is_some() {
            warn!("Field 'decoder' in video options is deprecated. The codec will now be set automatically based on WHIP negotiation, manual specification is no longer needed.")
        }

        // TODO: move this logic to pipeline and resolve the final values
        // when we know if vulkan decoder is supported
        let whip_options = match video {
            Some(options) => {
                let video_preferences = match options.decoder_preferences.as_deref() {
                    Some([]) | None => vec![WhipServerVideoDecoder::Any],
                    Some(v) => v.to_vec(),
                };
                let video_preferences: Vec<pipeline::VideoDecoderOptions> = video_preferences
                    .into_iter()
                    .flat_map(|codec| match codec {
                        WhipServerVideoDecoder::FfmpegH264 => {
                            vec![pipeline::VideoDecoderOptions::FfmpegH264]
                        }
                        #[cfg(feature = "vk-video")]
                        WhipServerVideoDecoder::VulkanH264 => {
                            vec![pipeline::VideoDecoderOptions::VulkanH264]
                        }
                        WhipServerVideoDecoder::FfmpegVp8 => {
                            vec![pipeline::VideoDecoderOptions::FfmpegVp8]
                        }
                        WhipServerVideoDecoder::FfmpegVp9 => {
                            vec![pipeline::VideoDecoderOptions::FfmpegVp9]
                        }
                        #[cfg(not(feature = "vk-video"))]
                        WhipServerVideoDecoder::Any => {
                            vec![
                                pipeline::VideoDecoderOptions::FfmpegVp9,
                                pipeline::VideoDecoderOptions::FfmpegVp8,
                                pipeline::VideoDecoderOptions::FfmpegH264,
                            ]
                        }
                        #[cfg(feature = "vk-video")]
                        WhipServerVideoDecoder::Any => {
                            vec![
                                pipeline::VideoDecoderOptions::FfmpegVp9,
                                pipeline::VideoDecoderOptions::FfmpegVp8,
                                pipeline::VideoDecoderOptions::VulkanH264,
                            ]
                        }
                        #[cfg(not(feature = "vk-video"))]
                        WhipServerVideoDecoder::VulkanH264 => vec![],
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
