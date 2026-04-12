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
            side_channel,
        } = value;

        let side_channel = side_channel.unwrap_or_default();

        let video_preferences = match video {
            Some(options) => match options.decoder_preferences.as_deref() {
                Some([]) | None => vec![core::WebrtcVideoDecoderOptions::Any],
                Some(v) => v.iter().copied().map(Into::into).collect(),
            },
            None => vec![core::WebrtcVideoDecoderOptions::Any],
        };

        let whep_options = core::WhepInputOptions {
            video_preferences,
            endpoint_url,
            bearer_token,
            queue_options: core::QueueInputOptions {
                required: required.unwrap_or(false),
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
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
