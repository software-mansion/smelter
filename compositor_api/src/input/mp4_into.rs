use std::collections::HashMap;
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
            video,
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

        let video_decoders = match video.and_then(|v| v.decoders) {
            Some(decoders) => decoders
                .into_iter()
                .map(|(codec, decoder)| {
                    let decoders = match (codec, decoder) {
                        (InputMp4VideoCodecs::H264, VideoDecoder::FfmpegH264) => (
                            pipeline::VideoCodec::H264,
                            pipeline::VideoDecoderOptions::FfmpegH264,
                        ),

                        #[cfg(feature = "vk-video")]
                        (
                            InputMp4VideoCodecs::H264,
                            VideoDecoder::VulkanH264 | VideoDecoder::VulkanVideo,
                        ) => (
                            pipeline::VideoCodec::H264,
                            pipeline::VideoDecoderOptions::VulkanH264,
                        ),

                        #[cfg(not(feature = "vk-video"))]
                        (
                            InputMp4VideoCodecs::H264,
                            VideoDecoder::VulkanVideo | VideoDecoder::VulkanH264,
                        ) => return Err(TypeError::new(super::NO_VULKAN_VIDEO)),

                        (InputMp4VideoCodecs::H264, _) => {
                            return Err(TypeError::new("Expected h264 decoder"))
                        }
                    };

                    Ok(decoders)
                })
                .collect::<Result<_, _>>()?,
            None => HashMap::new(),
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
