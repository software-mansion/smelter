use std::time::Duration;

use compositor_pipeline::{
    pipeline::{
        self,
        input::{self, whip},
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
            audio,
            required,
            offset_ms,
        } = value;

        if video.clone().and_then(|v| v.decoder.clone()).is_some() {
            warn!("Field 'decoder' in video options is deprecated. The codec will now be set automatically based on WHIP negotiation, manual specification is no longer needed.")
        }

        if audio.is_some() {
            warn!("Field 'audio' is deprecated. The codec will now be set automatically based on WHIP negotiation, manual specification is no longer needed.")
        }

        let whip_options = match video {
            Some(options) => {
                let video_decoder_preferences = match options.decoder_preferences.as_deref() {
                    Some([]) | None => vec![WhipVideoDecoder::Any],
                    Some(v) => v.to_vec(),
                };
                let video_decoder_preferences: Vec<pipeline::VideoDecoder> =
                    video_decoder_preferences
                        .into_iter()
                        .flat_map(|codec| match codec {
                            WhipVideoDecoder::FfmpegH264 => {
                                vec![pipeline::VideoDecoder::FFmpegH264]
                            }
                            #[cfg(feature = "vk-video")]
                            WhipVideoDecoder::VulkanH264 => {
                                vec![pipeline::VideoDecoder::VulkanVideoH264]
                            }
                            WhipVideoDecoder::FfmpegVp8 => {
                                vec![pipeline::VideoDecoder::FFmpegVp8]
                            }
                            WhipVideoDecoder::FfmpegVp9 => {
                                vec![pipeline::VideoDecoder::FFmpegVp9]
                            }
                            #[cfg(not(feature = "vk-video"))]
                            WhipVideoDecoder::Any => {
                                vec![
                                    pipeline::VideoDecoder::FFmpegVp9,
                                    pipeline::VideoDecoder::FFmpegVp8,
                                    pipeline::VideoDecoder::FFmpegH264,
                                ]
                            }
                            #[cfg(feature = "vk-video")]
                            WhipVideoDecoder::Any => {
                                vec![
                                    pipeline::VideoDecoder::FFmpegVp9,
                                    pipeline::VideoDecoder::FFmpegVp8,
                                    pipeline::VideoDecoder::VulkanVideoH264,
                                ]
                            }
                            #[cfg(not(feature = "vk-video"))]
                            WhipVideoDecoder::VulkanH264 => vec![],
                        })
                        .unique()
                        .collect();
                whip::WhipOptions {
                    video_decoder_preferences,
                }
            }
            None => whip::WhipOptions {
                video_decoder_preferences: vec![
                    pipeline::VideoDecoder::FFmpegVp9,
                    pipeline::VideoDecoder::FFmpegVp8,
                    pipeline::VideoDecoder::FFmpegH264,
                ],
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
