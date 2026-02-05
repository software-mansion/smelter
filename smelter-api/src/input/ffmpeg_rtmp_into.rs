use crate::common_core::prelude as core;
use crate::*;
use std::time::Duration;

impl TryFrom<FfmpegRtmpInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: FfmpegRtmpInput) -> Result<Self, Self::Error> {
        let FfmpegRtmpInput {
            url,
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
            .and_then(|decoders| decoders.get(&InputFfmpegRtmpCodec::H264))
            .map(|decoder| match decoder {
                FfmpegRtmpVideoDecoderOptions::FfmpegH264 => {
                    Ok(core::VideoDecoderOptions::FfmpegH264)
                }
                FfmpegRtmpVideoDecoderOptions::VulkanH264 => {
                    Ok(core::VideoDecoderOptions::VulkanH264)
                }
            })
            .transpose()?;

        let video_decoders = core::RtmpServerInputVideoDecoders { h264 };

        let input_options = core::FfmpegRtmpServerInputOptions {
            url,
            video_decoders,
            buffer,
        };

        Ok(core::RegisterInputOptions {
            input_options: core::ProtocolInputOptions::FfmpegRtmpServer(input_options),
            queue_options,
        })
    }
}
