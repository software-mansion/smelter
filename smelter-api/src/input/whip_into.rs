use std::time::Duration;

use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<WhipInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhipInput) -> Result<Self, Self::Error> {
        let WhipInput {
            video,
            required,
            offset_ms,
            bearer_token,
            endpoint_override,
        } = value;

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
        };

        let input_options = core::ProtocolInputOptions::Whip(whip_options);

        let queue_options = smelter_core::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        Ok(core::RegisterInputOptions {
            input_options,
            queue_options,
        })
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
