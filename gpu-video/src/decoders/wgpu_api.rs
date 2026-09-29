use std::time::Duration;

use crate::{DecoderEvent, EncodedInputChunk, VideoDecoderError, parser::h264::AccessUnit};

pub(crate) trait WgpuVideoDecoderBackend: Send {
    fn process_event_textures(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError>;
}

/// A decoder that outputs frames stored as [`wgpu::Texture`]s
pub struct WgpuTexturesDecoderH264 {
    pub(crate) backend: Box<dyn WgpuVideoDecoderBackend>,
}

impl WgpuTexturesDecoderH264 {
    /// The decoded frames are sent via the callback provided at creation. The produced textures
    /// have the [`wgpu::TextureFormat::NV12`] format and can be used as a texture binding.
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
            .process_event_textures(event, timeout.unwrap_or(Duration::MAX))
    }
}
