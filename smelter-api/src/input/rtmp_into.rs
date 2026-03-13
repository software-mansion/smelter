use crate::common_core::prelude as core;
use crate::*;
use std::time::Duration;

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

        let queue_options = smelter_core::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

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
                RtmpVideoDecoderOptions::FfmpegVp9 => {
                    Err(TypeError::new("Cannot use VP9 decoder for H264 codec."))
                }
            })
            .transpose()?;

        let vp9 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputRtmpCodec::Vp9))
            .map(|decoder| match decoder {
                RtmpVideoDecoderOptions::FfmpegVp9 => Ok(core::VideoDecoderOptions::FfmpegVp9),
                _ => Err(TypeError::new("Cannot use H264 decoder for VP9 codec.")),
            })
            .transpose()?;

        let input_options = core::RtmpServerInputOptions {
            app,
            stream_key,
            decoders: core::RtmpServerInputDecoders { h264, vp9 },
            buffer,
        };

        Ok(core::RegisterInputOptions {
            input_options: core::ProtocolInputOptions::RtmpServer(input_options),
            queue_options,
        })
    }
}
