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
            offset_ms,
        } = value;

        let queue_options = smelter_core::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        let jitter_buffer = match &queue_options {
            core::QueueInputOptions {
                required: false,
                offset: None,
            } => core::RtpJitterBufferOptions {
                mode: core::RtpJitterBufferMode::QueueBased,
                buffer: core::InputBufferOptions::LatencyOptimized,
            },
            _ => core::RtpJitterBufferOptions {
                mode: core::RtpJitterBufferMode::Fixed(Duration::from_millis(200)),
                buffer: core::InputBufferOptions::None,
            },
        };
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
            jitter_buffer,
        };

        let input_options = core::ProtocolInputOptions::Whep(whep_options);

        Ok(core::RegisterInputOptions {
            input_options,
            queue_options,
        })
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
