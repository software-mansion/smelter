#![cfg(feature = "decklink")]

use decklink::PixelFormat;

#[derive(Debug, Clone)]
pub struct DeckLinkInputOptions {
    pub subdevice_index: Option<u32>,
    pub display_name: Option<String>,
    /// Persistent id of a device (different value for each sub-device).
    pub persistent_id: Option<u32>,

    pub enable_audio: bool,
    /// Force specified pixel format, value resolved in input format
    /// autodetection will be ignored.
    pub pixel_format: Option<PixelFormat>,
}


