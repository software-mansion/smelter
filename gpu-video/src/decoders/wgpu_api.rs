use crate::{
    DecoderEvent, EncodedInputChunk, VideoDecoderError, decoders::VideoDecoderBackend,
    parser::h264::AccessUnit,
};

/// A decoder that outputs frames stored as [`wgpu::Texture`]s
pub struct WgpuTexturesDecoderH264 {
    pub(crate) backend: Box<dyn VideoDecoderBackend>,
}

impl WgpuTexturesDecoderH264 {
    /// Decodes the chunk without blocking. The decoded frames are sent via the provided callback.
    /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as a texture binding.
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
