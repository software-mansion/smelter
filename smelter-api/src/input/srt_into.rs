use crate::common_core::prelude as core;
use crate::*;

use super::queue_options::new_queue_options;

impl TryFrom<SrtInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: SrtInput) -> Result<Self, Self::Error> {
        let SrtInput {
            port,
            video,
            audio,
            required,
            offset_ms,
            side_channel,
        } = value;

        const NO_VIDEO_AUDIO_MESSAGE: &str =
            "At least one of `video` and `audio` has to be specified in `register_input` request.";
        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(NO_VIDEO_AUDIO_MESSAGE));
        }

        let (required, offset) = new_queue_options(required, offset_ms)?;
        let side_channel = side_channel.unwrap_or_default();

        let video = video.map(|v| {
            let decoder = match v.decoder {
                SrtVideoDecoderOptions::FfmpegH264 => core::VideoDecoderOptions::FfmpegH264,
                SrtVideoDecoderOptions::VulkanH264 => core::VideoDecoderOptions::VulkanH264,
            };
            core::SrtInputVideoOptions { decoder }
        });

        let audio = audio.and_then(|a| a.then_some(core::SrtInputAudioOptions::Aac));

        Ok(core::RegisterInputOptions::Srt(core::SrtInputOptions {
            port,
            video,
            audio,
            queue_options: core::QueueInputOptions {
                required,
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
            },
            offset,
        }))
    }
}
