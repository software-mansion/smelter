use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<DeckLink> for core::RegisterInputOptions {
    type Error = TypeError;

    #[cfg(feature = "decklink")]
    fn try_from(value: DeckLink) -> Result<Self, Self::Error> {
        const ID_PARSE_ERROR_MESSAGE: &str =
            "\"persistent_id\" has to be a valid 32-bit hexadecimal number";

        let persistent_id = match value.persistent_id {
            Some(persistent_id) => {
                let Ok(persistent_id) = u32::from_str_radix(&persistent_id, 16) else {
                    return Err(TypeError::new(ID_PARSE_ERROR_MESSAGE));
                };
                Some(persistent_id)
            }
            None => None,
        };

        Ok(core::RegisterInputOptions::DeckLink(
            core::DeckLinkInputOptions {
                subdevice_index: value.subdevice_index,
                display_name: value.display_name,
                persistent_id,
                enable_audio: value.enable_audio.unwrap_or(true),
                pixel_format: Some(core::DeckLinkPixelFormat::Format8BitYUV),
                queue_options: {
                    let side_channel = value.side_channel.unwrap_or_default();
                    core::QueueInputOptions {
                        required: value.required.unwrap_or(false),
                        video_side_channel: side_channel.video.unwrap_or(false),
                        audio_side_channel: side_channel.audio.unwrap_or(false),
                    }
                },
            },
        ))
    }

    #[cfg(not(feature = "decklink"))]
    fn try_from(_value: DeckLink) -> Result<Self, Self::Error> {
        Err(TypeError::new(
            "This Smelter binary was build without DeckLink support. Rebuilt it with \"decklink\" feature enabled.",
        ))
    }
}
