use std::time::Duration;

use compositor_pipeline::{
    pipeline::{
        self, decoder,
        input::{self},
        webrtc,
    },
    queue,
};
use itertools::Itertools;
use tracing::warn;

use crate::*;

impl TryFrom<WhipInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhipInput) -> Result<Self, Self::Error> {
        let WhipInput {
            video,
            audio: _,
            required,
            offset_ms,
            bearer_token,
        } = value;

        if video.clone().and_then(|v| v.decoder.clone()).is_some() {
            warn!("Field 'decoder' in video options is deprecated. The codec will now be set automatically based on WHIP negotiation, manual specification is no longer needed.")
        }

        // TODO: move this logic to pipeline and resolve the final values
        // when we know if vulkan decoder is supported
        let whip_options = match video {
            Some(options) => {
                let video_preferences = match options.decoder_preferences.as_deref() {
                    Some([]) | None => vec![WhipVideoDecoder::Any],
                    Some(v) => v.to_vec(),
                };
                let video_preferences: Vec<decoder::VideoDecoderOptions> = video_preferences
                    .into_iter()
                    .flat_map(|codec| match codec {
                        WhipVideoDecoder::FfmpegH264 => {
                            vec![decoder::VideoDecoderOptions::FfmpegH264]
                        }
                        #[cfg(feature = "vk-video")]
                        WhipVideoDecoder::VulkanH264 => {
                            vec![decoder::VideoDecoderOptions::VulkanH264]
                        }
                        WhipVideoDecoder::FfmpegVp8 => {
                            vec![decoder::VideoDecoderOptions::FfmpegVp8]
                        }
                        WhipVideoDecoder::FfmpegVp9 => {
                            vec![decoder::VideoDecoderOptions::FfmpegVp9]
                        }
                        #[cfg(not(feature = "vk-video"))]
                        WhipVideoDecoder::Any => {
                            vec![
                                decoder::VideoDecoderOptions::FfmpegVp9,
                                decoder::VideoDecoderOptions::FfmpegVp8,
                                decoder::VideoDecoderOptions::FfmpegH264,
                            ]
                        }
                        #[cfg(feature = "vk-video")]
                        WhipVideoDecoder::Any => {
                            vec![
                                decoder::VideoDecoderOptions::FfmpegVp9,
                                decoder::VideoDecoderOptions::FfmpegVp8,
                                decoder::VideoDecoderOptions::VulkanH264,
                            ]
                        }
                        #[cfg(not(feature = "vk-video"))]
                        WhipVideoDecoder::VulkanH264 => vec![],
                    })
                    .unique()
                    .collect();
                webrtc::WhipInputOptions {
                    video_preferences,
                    bearer_token,
                }
            }
            None => webrtc::WhipInputOptions {
                #[cfg(not(feature = "vk-video"))]
                video_preferences: vec![
                    decoder::VideoDecoderOptions::FfmpegH264,
                    decoder::VideoDecoderOptions::FfmpegVp8,
                    decoder::VideoDecoderOptions::FfmpegVp9,
                ],
                #[cfg(feature = "vk-video")]
                video_preferences: vec![
                    decoder::VideoDecoderOptions::VulkanH264,
                    decoder::VideoDecoderOptions::FfmpegH264,
                    decoder::VideoDecoderOptions::FfmpegVp8,
                    decoder::VideoDecoderOptions::FfmpegVp9,
                ],
                bearer_token,
            },
        };

        let input_options = input::InputOptions::Whip(whip_options);

        let queue_options = queue::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        Ok(pipeline::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}
