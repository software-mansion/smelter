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

        Ok(core::RegisterInputOptions {
            input_options: core::ProtocolInputOptions::DeckLink(core::DeckLinkInputOptions {
                subdevice_index: value.subdevice_index,
                display_name: value.display_name,
                persistent_id,
                enable_audio: value.enable_audio.unwrap_or(true),
                pixel_format: Some(core::DeckLinkPixelFormat::Format8BitYUV),
            }),
            queue_options: core::QueueInputOptions {
                required: value.required.unwrap_or(false),
                offset: None,
            },
        })
    }

    #[cfg(not(feature = "decklink"))]
    fn try_from(_value: DeckLink) -> Result<Self, Self::Error> {
        Err(TypeError::new(
            "This Smelter binary was build without DeckLink support. Rebuilt it with \"decklink\" feature enabled.",
        ))
    }
}
