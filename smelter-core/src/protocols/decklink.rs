pub use decklink::PixelFormat as DeckLinkPixelFormat;

#[derive(Debug, Clone)]
pub struct DeckLinkInputOptions {
    pub subdevice_index: Option<u32>,
    pub display_name: Option<String>,
    /// Persistent id of a device (different value for each sub-device).
    pub persistent_id: Option<u32>,

    pub enable_audio: bool,
    /// Force specified pixel format, value resolved in input format
    /// autodetection will be ignored.
    pub pixel_format: Option<DeckLinkPixelFormat>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeckLinkInputError {
    #[error("Unknown DeckLink error.")]
    DecklinkError(#[from] decklink::DeckLinkError),
    #[error("No DeckLink device matches specified options. Found devices: {0:?}")]
    NoMatchingDeckLink(Vec<DeckLinkDeviceInfo>),
    #[error("Selected device does not support capture.")]
    NoCaptureSupport,
    #[error("Selected device does not support input format detection.")]
    NoInputFormatDetection,
}

#[derive(Debug)]
pub struct DeckLinkDeviceInfo {
    pub display_name: Option<String>,
    pub persistent_id: Option<String>,
    pub subdevice_index: Option<u32>,
}
