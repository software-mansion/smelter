use std::time::Duration;

use crate::common_pipeline::prelude as pipeline;
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
            decoder_map,
        } = value;

        const BAD_URL_PATH_SPEC: &str =
            "Exactly one of `url` or `path` has to be specified in a register request for an mp4 input.";

        let source = match (url, path) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(TypeError::new(BAD_URL_PATH_SPEC));
            }
            (Some(url), None) => pipeline::Mp4InputSource::Url(url),
            (None, Some(path)) => pipeline::Mp4InputSource::File(path.into()),
        };

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        let video_decoders = match decoder_map {
            Some(decoders) => {
                let h264 = decoders
                    .get(&InputMp4Codec::H264)
                    .map(|decoder| match decoder {
                        VideoDecoder::FfmpegH264 => Ok(pipeline::VideoDecoderOptions::FfmpegH264),

                        #[cfg(feature = "vk-video")]
                        VideoDecoder::VulkanH264 | VideoDecoder::VulkanVideo => {
                            Ok(pipeline::VideoDecoderOptions::VulkanH264)
                        }

                        #[cfg(not(feature = "vk-video"))]
                        VideoDecoder::VulkanVideo | VideoDecoder::VulkanH264 => {
                            Err(TypeError::new(super::NO_VULKAN_VIDEO))
                        }

                        _ => Err(TypeError::new("Expected h264 decoder")),
                    })
                    .unwrap_or(Ok(pipeline::VideoDecoderOptions::FfmpegH264))?;

                pipeline::Mp4InputVideoDecoders { h264 }
            }
            None => pipeline::Mp4InputVideoDecoders {
                h264: pipeline::VideoDecoderOptions::FfmpegH264,
            },
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: pipeline::ProtocolInputOptions::Mp4(pipeline::Mp4InputOptions {
                source,
                should_loop: should_loop.unwrap_or(false),
                video_decoders,
            }),
            queue_options,
        })
    }
}
