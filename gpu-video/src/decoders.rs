use std::sync::{Arc, Mutex};

use crate::{
    DecoderEvent, EncodedInputChunk, H264ParserError, OutputFrame, ReferenceManagementError,
    VideoBackendError, parser::h264::AccessUnit,
};

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

pub(crate) type FrameCallback<T> =
    Arc<Mutex<dyn FnMut(Result<OutputFrame<T>, VideoDecoderError>) + Send>>;

pub(crate) trait VideoDecoderBackend: Send {
    fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<(), VideoDecoderError>;
}

/// A decoder that outputs frames stored as [`Vec<u8>`] with the raw pixel data.
pub struct BytesDecoderH264 {
    pub(crate) backend: Box<dyn VideoDecoderBackend>,
}

impl BytesDecoderH264 {
    /// Decodes the chunk without blocking. The decoded frames are sent via the provided callback.
    /// The decoded frames are represented by [`Vec<u8>`] in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    ///
    /// If [`DecoderParameters::max_in_flight_submissions`](crate::parameters::DecoderParameters::max_in_flight_submissions)
    /// decode submissions are already in flight, this blocks until the oldest one finishes.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn decode(&mut self, frame: EncodedInputChunk<'_>) -> Result<(), VideoDecoderError> {
        self.process_event(DecoderEvent::DecodeChunk(frame))
    }

    /// Flush all frames from the decoder.
    /// This blocks until all frames have been sent via the provided callback.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoDecoderError> {
        self.process_event(DecoderEvent::Flush)
    }

    /// Process a [`DecoderEvent`]. For most use cases, using [`Self::decode`] and [`Self::flush`] is enough.
    /// Use this only when you need more fine-grained control.
    ///
    /// Depending on the event this may block until the event is processed completely,
    /// or return early and process the event in the background.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<(), VideoDecoderError> {
        self.backend.process_event(event)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VideoDecoderError {
    #[error("The device does not support decoding")]
    DecoderUnsupported,

    #[error("Invalid input data for the decoder: {0}.")]
    InvalidInputData(String),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] H264ParserError),

    #[error("Reference management error: {0}")]
    ReferenceManagementError(#[from] ReferenceManagementError),

    #[cfg(feature = "wgpu")]
    #[error(
        "VideoDevice was created without wgpu support. Initialize wgpu::Device using VideoAdapterExt::request_device_with_video_support"
    )]
    VideoDeviceWithoutWgpu,

    #[error("Encoder error: {0}")]
    BackendError(VideoBackendError),
}
