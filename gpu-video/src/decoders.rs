use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{
    DecoderEvent, EncodedInputChunk, H264ParserError, ReferenceManagementError, VideoBackendError,
    device::CorruptedStateHandling,
    parser::{
        decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
        h264::{AccessUnit, H264Parser},
        reference_manager::ReferenceContext,
    },
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

/// Turns decoder events into decoder instructions and keeps the parser and reference state.
///
/// Backends set the shared `decode_failed` flag from their completion threads when a submitted
/// frame fails to decode. The flag is consumed at the start of the next event, which marks the
/// reference state as corrupted.
pub(crate) struct H264EventProcessor {
    parser: H264Parser,
    reference_ctx: ReferenceContext,
    decode_failed: Arc<AtomicBool>,
}

impl H264EventProcessor {
    pub(crate) fn new(
        parser: H264Parser,
        corrupted_state_handling: CorruptedStateHandling,
    ) -> Self {
        Self {
            parser,
            reference_ctx: ReferenceContext::new(corrupted_state_handling),
            decode_failed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn decode_failed_flag(&self) -> Arc<AtomicBool> {
        self.decode_failed.clone()
    }

    pub(crate) fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<DecoderInstruction>, VideoDecoderError> {
        if self.decode_failed.swap(false, Ordering::Relaxed) {
            self.reference_ctx.mark_corrupted_state();
        }

        let access_units = match event {
            DecoderEvent::DecodeChunk(chunk) => self.parser.parse(chunk.data, chunk.pts)?,
            DecoderEvent::DecodeParsedFrame(au) => vec![au],
            DecoderEvent::SignalFrameEnd | DecoderEvent::Flush => self.parser.flush()?,
            DecoderEvent::SignalDataLoss => {
                self.reference_ctx.mark_corrupted_state();
                return Ok(Vec::new());
            }
        };

        Ok(compile_to_decoder_instructions(
            &mut self.reference_ctx,
            access_units,
        )?)
    }
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
