use crate::common_core::prelude as core;
use crate::*;

use super::queue_options::new_queue_options;

impl TryFrom<SrtInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: SrtInput) -> Result<Self, Self::Error> {
        let SrtInput {
            video,
            audio,
            required,
            offset_ms,
            side_channel,
            encryption,
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

        let encryption = encryption
            .map(|e| {
                let passphrase_len = e.passphrase.len();
                if !(10..=79).contains(&passphrase_len) {
                    return Err(TypeError::new(
                        "SRT encryption passphrase must be between 10 and 79 characters long.",
                    ));
                }
                let key_length = match e.encryption {
                    SrtEncryption::Aes128 => core::SrtEncryptionKeyLength::Aes128,
                    SrtEncryption::Aes192 => core::SrtEncryptionKeyLength::Aes192,
                    SrtEncryption::Aes256 => core::SrtEncryptionKeyLength::Aes256,
                };
                Ok(core::SrtInputEncryption {
                    passphrase: e.passphrase,
                    key_length,
                })
            })
            .transpose()?;

        Ok(core::RegisterInputOptions::Srt(core::SrtInputOptions {
            video,
            audio,
            queue_options: core::QueueInputOptions {
                required,
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
            },
            offset,
            encryption,
        }))
    }
}
