use std::{num::NonZero, sync::Arc};

use ash::vk;

use crate::{
    DecoderError, EncodedInputChunk, VulkanCommonError, VulkanDevice, VulkanEncoderError,
    device::{DecodingDevice, EncoderParameters, Rational, VideoParameters},
    parser::{
        decoder_instructions::compile_to_decoder_instructions, h264::H264Parser,
        reference_manager::ReferenceContext,
    },
    vulkan_decoder::{FrameSorter, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
    vulkan_transcoder::pipeline::{OutputConfig, Pipeline},
};

mod pipeline;

struct SingleBlitImage {}

#[derive(Debug, thiserror::Error)]
pub enum TranscoderError {
    #[error(transparent)]
    DecoderError(#[from] DecoderError),

    #[error(transparent)]
    VulkanEncoderError(#[from] VulkanEncoderError),

    #[error(transparent)]
    VulkanCommonError(#[from] VulkanCommonError),

    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),
}

pub struct Transcoder {
    decoder: VulkanDecoder<'static>,
    parser: H264Parser,
    reference_ctx: ReferenceContext,
    sorter: FrameSorter<()>,
    pipeline: Pipeline,
    encoders: Vec<VulkanEncoder<'static>>,
}

impl Transcoder {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        parameters: &[EncoderParameters],
    ) -> Result<Self, TranscoderError> {
        let decoder = VulkanDecoder::new(
            Arc::new(
                device
                    .decoding_device()
                    .map_err(DecoderError::VulkanDecoderError)?,
            ),
            vk::VideoDecodeUsageFlagsKHR::TRANSCODING,
        )
        .map_err(DecoderError::VulkanDecoderError)?;

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::default();
        let sorter = FrameSorter::new();

        let mut encoders = parameters
            .iter()
            .map(|p| {
                let parameters = device.validate_and_fill_encoder_parameters(p.clone());
                parameters.and_then(|p| VulkanEncoder::new(Arc::new(device.encoding_device()?), p))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let output_configs = encoders
            .iter_mut()
            .zip(parameters.iter())
            .map(|(e, p)| OutputConfig {
                width: p.video_parameters.width.get(),
                height: p.video_parameters.height.get(),
                tracker: &mut e.tracker,
                profile: &e.profile_info,
            })
            .collect::<Vec<_>>();

        let compute_pipeline = pipeline::Pipeline::new(device.clone(), &output_configs)?;

        Ok(Self {
            decoder,
            parser,
            reference_ctx,
            sorter,
            pipeline: compute_pipeline,
            encoders: Vec::new(),
        })
    }

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
