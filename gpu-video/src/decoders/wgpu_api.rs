use crate::{
    DecoderEvent, EncodedInputChunk, OutputFrame, VideoDecoderError,
    frame_sorter::{DecodeResult, FrameSorter},
    parser::{
        decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
        h264::{AccessUnit, H264Parser},
        reference_manager::ReferenceContext,
    },
};

pub(crate) trait WgpuVideoDecoderBackend: Send {
    fn decode_to_wgpu_textures(
        &mut self,
        wgpu_device: &wgpu::Device,
        decoder_instructions: &[DecoderInstruction],
    ) -> Result<Vec<DecodeResult<wgpu::Texture>>, VideoDecoderError>;
}

/// A decoder that outputs frames stored as [`wgpu::Texture`]s
pub struct WgpuTexturesDecoder {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) decoder: Box<dyn WgpuVideoDecoderBackend>,
    pub(crate) parser: H264Parser,
    pub(crate) reference_ctx: ReferenceContext,
    pub(crate) frame_sorter: FrameSorter<wgpu::Texture>,
}

impl WgpuTexturesDecoder {
    /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as a texture binding.
    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<'_>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VideoDecoderError> {
        self.process_event(DecoderEvent::DecodeChunk(frame))
    }

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    pub fn flush(&mut self) -> Result<Vec<OutputFrame<wgpu::Texture>>, VideoDecoderError> {
        self.process_event(DecoderEvent::Flush)
    }

    /// Process a [`DecoderEvent`]. For most use cases, using [`Self::decode`] and [`Self::flush`] is enough.
    /// Use this only when you need more fine-grained control.
    /// May return a sequence of decoded frames in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    pub fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VideoDecoderError> {
        match event {
            DecoderEvent::DecodeChunk(chunk) => {
                let nalus = self.parser.parse(chunk.data, chunk.pts)?;
                self.decode_access_units(nalus)
            }
            DecoderEvent::DecodeParsedFrame(au) => self.decode_access_units(vec![au]),
            DecoderEvent::SignalFrameEnd => {
                let access_units = self.parser.flush()?;
                self.decode_access_units(access_units)
            }
            DecoderEvent::SignalDataLoss => {
                self.reference_ctx.mark_missed_frames();
                Ok(Vec::new())
            }
            DecoderEvent::Flush => {
                let access_units = self.parser.flush()?;
                let mut frames = self.decode_access_units(access_units)?;
                frames.append(&mut self.frame_sorter.flush());
                Ok(frames)
            }
        }
    }

    fn decode_access_units(
        &mut self,
        access_units: Vec<AccessUnit>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VideoDecoderError> {
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;
        let unsorted_frames = self
            .decoder
            .decode_to_wgpu_textures(&self.wgpu_device, &instructions)?;
        let sorted_frames = self.frame_sorter.put_frames(unsorted_frames);
        Ok(sorted_frames)
    }
}
