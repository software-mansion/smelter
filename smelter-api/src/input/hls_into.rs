use crate::common_core::prelude as core;
use crate::*;

use super::queue_options::new_queue_options;

impl TryFrom<HlsInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: HlsInput) -> Result<Self, Self::Error> {
        let HlsInput {
            url,
            required,
            offset_ms,
            decoder_map,
        } = value;

        let (required, offset) = new_queue_options(required, offset_ms)?;

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputHlsCodec::H264))
            .map(|decoder| match decoder {
                HlsVideoDecoderOptions::FfmpegH264 => Ok(core::VideoDecoderOptions::FfmpegH264),
                HlsVideoDecoderOptions::VulkanH264 => Ok(core::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let video_decoders = core::HlsInputVideoDecoders { h264 };

        let input_options = core::HlsInputOptions {
            url,
            video_decoders,
            required,
            offset,
        };

        Ok(core::RegisterInputOptions::Hls(input_options))
    }
}
