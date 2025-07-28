use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<DeckLink> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    #[cfg(feature = "decklink")]
    fn try_from(value: DeckLink) -> Result<Self, Self::Error> {
        use std::time::Duration;

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

        Ok(pipeline::RegisterInputOptions {
            input_options: pipeline::ProtocolInputOptions::DeckLink(
                pipeline::DeckLinkInputOptions {
                    subdevice_index: value.subdevice_index,
                    display_name: value.display_name,
                    persistent_id,
                    enable_audio: value.enable_audio.unwrap_or(true),
                    pixel_format: Some(pipeline::DeckLinkPixelFormat::Format8BitYUV),
                },
            ),
            queue_options: pipeline::QueueInputOptions {
                required: value.required.unwrap_or(false),
                offset: None,
                buffer_duration: Some(Duration::from_millis(5)),
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
