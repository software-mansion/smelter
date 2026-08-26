use std::time::Duration;

use crate::{
    DecoderEvent, EncodedInputChunk, H264ParserError, ReferenceManagementError, VideoBackendError,
    parser::h264::AccessUnit,
};

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

pub(crate) trait VideoDecoderBackend: Send {
    fn process_event_bytes(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError>;
}

/// A decoder that outputs frames stored as [`Vec<u8>`] with the raw pixel data.
pub struct BytesDecoderH264 {
    pub(crate) backend: Box<dyn VideoDecoderBackend>,
}

impl BytesDecoderH264 {
    /// The decoded frames are sent via the callback provided at creation.
    ///
    /// If [`DecoderParameters::max_in_flight_submissions`](crate::parameters::DecoderParameters::max_in_flight_submissions)
    /// decode submissions are already in flight, this blocks until all submissions above the limit finish.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn decode(&mut self, frame: EncodedInputChunk<'_>) -> Result<(), VideoDecoderError> {
        self.process_event(DecoderEvent::DecodeChunk(frame), None)
    }

    /// Flush all frames from the decoder.
    /// This blocks until all frames have been sent via the provided callback.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoDecoderError> {
        self.process_event(DecoderEvent::Flush, None)
    }

    /// Process a [`DecoderEvent`]. For most use cases, using [`Self::decode`] and [`Self::flush`] is enough.
    /// Use this only when you need more fine-grained control.
    ///
    /// If the provided event does any decoding operation and [`DecoderParameters::max_in_flight_submissions`](crate::parameters::DecoderParameters::max_in_flight_submissions)
    /// decode submissions are already in flight, this blocks until all submissions above the limit finish, or times out after `timeout`.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Option<Duration>,
    ) -> Result<(), VideoDecoderError> {
        self.backend
            .process_event_bytes(event, timeout.unwrap_or(Duration::MAX))
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

    #[error("Decode submission timed out")]
    DecodeSubmissionTimeout,

    #[error("Decoder error: {0}")]
    BackendError(VideoBackendError),
}
