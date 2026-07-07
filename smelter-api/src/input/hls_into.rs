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
            side_channel,
            buffer,
        } = value;

        let (required, offset) = new_queue_options(required, offset_ms)?;
        let side_channel = side_channel.unwrap_or_default();
        let side_channel_delay = side_channel.delay()?;

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputHlsCodec::H264))
            .map(|decoder| match decoder {
                HlsVideoDecoderOptions::FfmpegH264 => Ok(core::VideoDecoderOptions::FfmpegH264),
                HlsVideoDecoderOptions::VulkanH264 => Ok(core::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let input_options = core::HlsInputOptions {
            url,
            decoder_options: core::HlsInputDecoders { h264 },
            queue_options: core::QueueInputOptions {
                required,
                video_side_channel: side_channel.video.unwrap_or(false).into(),
                audio_side_channel: side_channel.audio.unwrap_or(false).into(),
                side_channel_delay,
            },
            offset,
            buffer: buffer
                .map(TryInto::try_into)
                .transpose()?
                .unwrap_or_default(),
        };

        Ok(core::RegisterInputOptions::Hls(input_options))
    }
}
