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

        let queue_options = new_queue_options(required, offset_ms)?;

        let buffer = match &queue_options {
            core::QueueInputOptions {
                required: false,
                offset: None,
            } => core::InputBufferOptions::Const(None),
            _ => core::InputBufferOptions::None,
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
        };

        Ok(core::RegisterInputOptions {
            input_options: core::ProtocolInputOptions::RtmpServer(input_options),
            queue_options,
        })
    }
}
