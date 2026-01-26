use crate::{
    DecoderError, EncodedInputChunk, VulkanEncoderError,
    parser::{
        decoder_instructions::compile_to_decoder_instructions, h264::H264Parser,
        reference_manager::ReferenceContext,
    },
    vulkan_decoder::{FrameSorter, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
};

struct SingleBlitImage {}

#[derive(Debug, thiserror::Error)]
enum TranscoderError {
    #[error(transparent)]
    DecoderError(#[from] DecoderError),

    #[error(transparent)]
    VulkanEncoderError(#[from] VulkanEncoderError),
}

struct Transcoder {
    decoder: VulkanDecoder<'static>,
    parser: H264Parser,
    reference_ctx: ReferenceContext,
    sorter: FrameSorter<()>,

    encoders: Vec<VulkanEncoder<'static>>,
}

impl Transcoder {
    pub fn transcode(&mut self, input: EncodedInputChunk<&[u8]>) -> Result<(), TranscoderError> {
        let access_units = self
            .parser
            .parse(input.data, input.pts)
            .map_err(DecoderError::from)?;

        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)
            .map_err(DecoderError::from)?;

        for instruction in instructions {
            let Some(frame) = self
                .decoder
                .decode(&instruction)
                .map_err(DecoderError::from)?
            else {
                continue;
            };


        }

        Ok(())
    }
}

struct ComputePipeline {

}
