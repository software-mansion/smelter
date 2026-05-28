use std::time::Duration;

use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<MoqInputServer> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: MoqInputServer) -> Result<Self, Self::Error> {
        let MoqInputServer {
            broadcast_path,
            required,
            decoder_map,
            side_channel,
        } = value;

        let side_channel = side_channel.unwrap_or_default();

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputMoqCodec::H264))
            .map(|decoder| match decoder {
                MoqVideoDecoderOptions::FfmpegH264 => Ok(core::VideoDecoderOptions::FfmpegH264),
                MoqVideoDecoderOptions::VulkanH264 => Ok(core::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let input_options = core::MoqServerInputOptions {
            broadcast_path,
            decoders: core::MoqServerInputDecoders { h264, aac: None },
            queue_options: core::QueueInputOptions {
                required: required.unwrap_or(false),
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
                // TODO: (@jbrs) check what that is.
                side_channel_delay: Duration::ZERO,
            },
        };

        Ok(core::RegisterInputOptions::MoqServer(input_options))
    }
}
