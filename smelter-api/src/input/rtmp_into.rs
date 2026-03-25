use crate::common_core::prelude as core;
use crate::*;

use super::queue_options::new_queue_options;

impl TryFrom<RtmpInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: RtmpInput) -> Result<Self, Self::Error> {
        let RtmpInput {
            app,
            stream_key,
            required,
            offset_ms,
            decoder_map,
        } = value;

        let (required, offset) = new_queue_options(required, offset_ms)?;

        let buffer = if !required && offset.is_none() {
            core::InputBufferOptions::Const(None)
        } else {
            core::InputBufferOptions::None
        };

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputRtmpCodec::H264))
            .map(|decoder| match decoder {
                RtmpVideoDecoderOptions::FfmpegH264 => Ok(core::VideoDecoderOptions::FfmpegH264),
                RtmpVideoDecoderOptions::VulkanH264 => Ok(core::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let input_options = core::RtmpServerInputOptions {
            app,
            stream_key,
            decoders: core::RtmpServerInputDecoders { h264 },
            buffer,
            required,
            offset,
        };

        Ok(core::RegisterInputOptions::RtmpServer(input_options))
    }
}
