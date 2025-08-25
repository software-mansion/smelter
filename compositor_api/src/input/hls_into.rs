use crate::common_pipeline::prelude as pipeline;
use crate::*;
use std::time::Duration;

impl TryFrom<HlsInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: HlsInput) -> Result<Self, Self::Error> {
        let HlsInput {
            url,
            required,
            offset_ms,
            decoder_map,
        } = value;

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        let video_decoders = match decoder_map {
            Some(decoders) => {
                let h264 = decoders
                    .get(&InputHlsCodec::H264)
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

                pipeline::HlsInputVideoDecoders { h264 }
            }
            None => pipeline::HlsInputVideoDecoders {
                h264: pipeline::VideoDecoderOptions::FfmpegH264,
            },
        };

        let input_options = pipeline::HlsInputOptions {
            url,
            video_decoders,
            buffer_duration: None,
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: pipeline::ProtocolInputOptions::Hls(input_options),
            queue_options,
        })
    }
}
