use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<MoqClientInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: MoqClientInput) -> Result<Self, Self::Error> {
        let MoqClientInput {
            endpoint_url,
            broadcast_path,
            required,
            decoder_map,
            side_channel,
            disable_tls_verification,
        } = value;

        let side_channel = side_channel.unwrap_or_default();
        let side_channel_delay = side_channel.delay()?;

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputMoqClientCodec::H264))
            .map(|decoder| match decoder {
                MoqClientVideoDecoderOptions::FfmpegH264 => {
                    Ok(core::VideoDecoderOptions::FfmpegH264)
                }
                MoqClientVideoDecoderOptions::VulkanH264 => {
                    Ok(core::VideoDecoderOptions::VulkanH264)
                }
            })
            .transpose()?;

        let input_options = core::MoqClientInputOptions {
            endpoint_url,
            broadcast_path,
            disable_tls_verification: disable_tls_verification.unwrap_or(false),
            decoders: core::MoqInputDecoders { h264 },
            queue_options: core::QueueInputOptions {
                required: required.unwrap_or(false),
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
                side_channel_delay,
            },
        };

        Ok(core::RegisterInputOptions::MoqClient(input_options))
    }
}
