use std::collections::HashMap;
use std::time::Duration;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<HlsInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: HlsInput) -> Result<Self, Self::Error> {
        let HlsInput {
            url,
            required,
            offset_ms,
            video,
        } = value;

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
                        (InputHlsVideoCodecs::H264, VideoDecoder::FfmpegH264) => (
                            pipeline::VideoCodec::H264,
                            pipeline::VideoDecoderOptions::FfmpegH264,
                        ),

                        #[cfg(feature = "vk-video")]
                        (
                            InputHlsVideoCodecs::H264,
                            VideoDecoder::VulkanH264 | VideoDecoder::VulkanVideo,
                        ) => (
                            pipeline::VideoCodec::H264,
                            pipeline::VideoDecoderOptions::VulkanH264,
                        ),

                        #[cfg(not(feature = "vk-video"))]
                        (
                            InputHlsVideoCodecs::H264,
                            VideoDecoder::VulkanVideo | VideoDecoder::VulkanH264,
                        ) => return Err(TypeError::new(super::NO_VULKAN_VIDEO)),

                        (InputHlsVideoCodecs::H264, _) => {
                            return Err(TypeError::new("Expected h264 decoder"))
                        }
                    };

                    Ok(decoders)
                })
                .collect::<Result<_, _>>()?,
            None => HashMap::new(),
        };

        let input_options = pipeline::HlsInputOptions {
            url,
            video_decoders,
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: pipeline::ProtocolInputOptions::Hls(input_options),
            queue_options,
        })
    }
}
