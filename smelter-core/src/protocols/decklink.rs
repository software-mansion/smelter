pub use decklink::PixelFormat as DeckLinkPixelFormat;

use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct DeckLinkInputOptions {
    pub subdevice_index: Option<u32>,
    pub display_name: Option<String>,
    /// Persistent id of a device (different value for each sub-device).
    pub persistent_id: Option<u32>,

    pub enable_audio: bool,
    /// Force specified pixel format, value resolved in input format
    /// autodetection will be ignored.
    pub pixel_format: Option<DeckLinkPixelFormat>,
    /// Capture into host memory the GPU copies from, so the CPU never copies
    /// frames. Requires a video side channel, a GPU that can read host memory,
    /// and 8-bit YUV or BGRA frames. Formats whose rows the GPU can't copy fail
    /// to enable, like an unsupported signal.
    pub zero_copy: bool,
    pub queue_options: QueueInputOptions,
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
    #[error("Zero-copy capture requires a video side channel.")]
    ZeroCopyWithoutSideChannel,
    #[error("Zero-copy capture requires a GPU that can read host memory.")]
    ZeroCopyUnsupportedByGpu,
    #[error("Zero-copy capture does not support {0:?} frames.")]
    ZeroCopyUnsupportedPixelFormat(DeckLinkPixelFormat),
}

#[derive(Debug)]
pub struct DeckLinkDeviceInfo {
    pub display_name: Option<String>,
    pub persistent_id: Option<String>,
    pub subdevice_index: Option<u32>,
}
