use std::time::Duration;

use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<WhepInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhepInput) -> Result<Self, Self::Error> {
        let WhepInput {
            endpoint_url,
            bearer_token,
            video,
            required,
            buffer_size_ms,
            side_channel,
        } = value;

        let side_channel = side_channel.unwrap_or_default();
        let side_channel_delay = side_channel.delay()?;

        let video_preferences = match video {
            Some(options) => match options.decoder_preferences.as_deref() {
                Some([]) | None => vec![core::WebrtcVideoDecoderOptions::Any],
                Some(v) => v.iter().copied().map(Into::into).collect(),
            },
            None => vec![core::WebrtcVideoDecoderOptions::Any],
        };

        let jitter_buffer_size = buffer_size_ms
            .map(|ms| Duration::try_from_secs_f64(ms / 1000.0))
            .transpose()
            .map_err(|err| TypeError::new(format!("Invalid buffer_size_ms. {err}")))?;

        let whep_options = core::WhepInputOptions {
            video_preferences,
            endpoint_url,
            bearer_token,
            jitter_buffer_size,
            queue_options: core::QueueInputOptions {
                required: required.unwrap_or(false),
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
                side_channel_delay,
            },
        };

        Ok(core::RegisterInputOptions::Whep(whep_options))
    }
}

impl From<WhepVideoDecoderOptions> for core::WebrtcVideoDecoderOptions {
    fn from(decoder: WhepVideoDecoderOptions) -> Self {
        match decoder {
            WhepVideoDecoderOptions::FfmpegH264 => core::WebrtcVideoDecoderOptions::FfmpegH264,
            WhepVideoDecoderOptions::FfmpegVp8 => core::WebrtcVideoDecoderOptions::FfmpegVp8,
            WhepVideoDecoderOptions::FfmpegVp9 => core::WebrtcVideoDecoderOptions::FfmpegVp9,
            WhepVideoDecoderOptions::VulkanH264 => core::WebrtcVideoDecoderOptions::VulkanH264,
            WhepVideoDecoderOptions::Any => core::WebrtcVideoDecoderOptions::Any,
        }
    }
}
