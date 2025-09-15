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

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputHlsCodec::H264))
            .map(|decoder| match decoder {
                HlsVideoDecoderOptions::FfmpegH264 => Ok(pipeline::VideoDecoderOptions::FfmpegH264),
                HlsVideoDecoderOptions::VulkanH264 => Ok(pipeline::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let video_decoders = pipeline::HlsInputVideoDecoders { h264 };

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
