use std::time::Duration;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<WhepInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhepInput) -> Result<Self, Self::Error> {
        let WhepInput {
            endpoint_url,
            bearer_token,
            video,
            // audio,
            required,
            offset_ms,
        } = value;

        let whep_options = pipeline::WhepInputOptions {
            endpoint_url,
            bearer_token,
            video: video.map(|v| v.decoder.into()),
            audio: audio.map(|a| a.into()),
        };

        let input_options = pipeline::ProtocolInputOptions::Whep(whep_options);

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        Ok(pipeline::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}

impl From<WhepVideoDecoderOptions> for pipeline::VideoDecoderOptions {
    fn from(decoder: WhepVideoDecoderOptions) -> Self {
        match decoder {
            WhepVideoDecoderOptions::FfmpegH264 => pipeline::VideoDecoderOptions::FfmpegH264,
            WhepVideoDecoderOptions::FfmpegVp8 => pipeline::VideoDecoderOptions::FfmpegVp8,
            WhepVideoDecoderOptions::FfmpegVp9 => pipeline::VideoDecoderOptions::FfmpegVp9,
            WhepVideoDecoderOptions::VulkanH264 => pipeline::VideoDecoderOptions::VulkanH264,
        }
    }
}

impl From<InputWhepAudioOptions> for pipeline::AudioDecoderOptions {
    fn from(audio: InputWhepAudioOptions) -> Self {
        match audio {
            InputWhepAudioOptions::Opus => pipeline::AudioDecoderOptions::Opus,
        }
    }
}
