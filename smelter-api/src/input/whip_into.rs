use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<WhipInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhipInput) -> Result<Self, Self::Error> {
        let WhipInput {
            video,
            required,
            bearer_token,
            endpoint_override,
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

        let whip_options = core::WhipInputOptions {
            video_preferences,
            bearer_token,
            endpoint_override,
            queue_options: core::QueueInputOptions {
                required: required.unwrap_or(false),
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
            },
        };

        Ok(core::RegisterInputOptions::Whip(whip_options))
    }
}

impl From<WhipVideoDecoderOptions> for core::WebrtcVideoDecoderOptions {
    fn from(decoder: WhipVideoDecoderOptions) -> Self {
        match decoder {
            WhipVideoDecoderOptions::FfmpegH264 => core::WebrtcVideoDecoderOptions::FfmpegH264,
            WhipVideoDecoderOptions::FfmpegVp8 => core::WebrtcVideoDecoderOptions::FfmpegVp8,
            WhipVideoDecoderOptions::FfmpegVp9 => core::WebrtcVideoDecoderOptions::FfmpegVp9,
            WhipVideoDecoderOptions::VulkanH264 => core::WebrtcVideoDecoderOptions::VulkanH264,
            WhipVideoDecoderOptions::Any => core::WebrtcVideoDecoderOptions::Any,
        }
    }
}
