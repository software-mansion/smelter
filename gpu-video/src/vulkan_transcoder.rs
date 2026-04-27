use std::{num::NonZeroU32, sync::Arc};

use ash::vk;

use crate::{
    DecoderError, EncodedInputChunk, EncodedOutputChunk, OutputFrame, VulkanCommonError,
    VulkanDevice, VulkanEncoderError,
    device::{EncoderOutputParameters, Rational},
    parameters::ScalingAlgorithm,
    parser::{
        decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
        h264::H264Parser,
        reference_manager::ReferenceContext,
    },
    vulkan_decoder::{
        DecodeResult, FrameSorter, ImageModifiers, InFlightDecodeResources, VulkanDecoder,
    },
    vulkan_encoder::{FullEncoderParameters, H264EncodeProfileInfo, VulkanEncoder},
    vulkan_transcoder::pipeline::{OutputConfig, ResizeSubmission, ResizingPipeline},
    wrappers::{DecodeInputBuffer, DecodingQueryPool, SemaphoreWaitValue},
};

mod pipeline;

#[derive(Debug, thiserror::Error)]
pub enum TranscoderError {
    #[error(transparent)]
    Decoder(#[from] DecoderError),

    #[error(transparent)]
    Encoder(#[from] VulkanEncoderError),

    #[error(transparent)]
    Common(#[from] VulkanCommonError),

    #[error("Vulkan error: {0}")]
    Vulkan(#[from] vk::Result),

    #[error("Wrong output number: expected a value between 0 and {expected_max}, found {actual}")]
    WrongOutputNumber { expected_max: usize, actual: usize },
}

/// Configuration for a transcoder
#[derive(Debug, Clone)]
pub struct TranscoderParameters {
    pub input_framerate: Rational,
    pub output_parameters: Vec<TranscoderOutputParameters>,
}

/// Configuration for a single transcoder output.
#[derive(Debug, Clone, Copy)]
pub struct TranscoderOutputParameters {
    pub encoder_parameters: EncoderOutputParameters,
    pub output_width: NonZeroU32,
    pub output_height: NonZeroU32,
    pub scaling_algorithm: ScalingAlgorithm,
}

pub(crate) struct ResizedImages {
    images: ResizeSubmission,
    decoder_wait_value: SemaphoreWaitValue,
    decode_query_pool: Option<Arc<DecodingQueryPool>>,
    input_buffer: DecodeInputBuffer,
    _in_flight_resources: InFlightDecodeResources,
}

pub struct Transcoder {
    device: Arc<VulkanDevice>,
    decoder: VulkanDecoder<'static>,
    parser: H264Parser,
    reference_ctx: ReferenceContext,
    sorter: FrameSorter<ResizedImages>,
    resizing_pipeline: ResizingPipeline,
    encoders: Vec<VulkanEncoder<'static>>,
}

impl Transcoder {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        config: TranscoderParameters,
    ) -> Result<Self, TranscoderError> {
        let decoder = VulkanDecoder::new(
            Arc::new(
                device
                    .decoding_device()
                    .map_err(DecoderError::VulkanDecoderError)?,
            ),
            vk::VideoDecodeUsageFlagsKHR::TRANSCODING,
            ImageModifiers {
                create_flags: vk::ImageCreateFlags::EXTENDED_USAGE
                    | vk::ImageCreateFlags::MUTABLE_FORMAT,
                usage_flags: vk::ImageUsageFlags::STORAGE,
                additional_queue_index: device.queues.compute.family_index,
            },
        )
        .map_err(DecoderError::VulkanDecoderError)?;

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::default();
        let sorter = FrameSorter::new();

        let scaling_algorithms: Vec<_> = config
            .output_parameters
            .iter()
            .map(|c| c.scaling_algorithm)
            .collect();

        let parameters = config
            .output_parameters
            .iter()
            .map(|c| {
                device.validate_and_fill_encoder_parameters(
                    c.encoder_parameters,
                    c.output_width,
                    c.output_height,
                    config.input_framerate,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let encoders = parameters
            .iter()
            .copied()
            .map(|p| VulkanEncoder::new(Arc::new(device.encoding_device()?), p))
            .collect::<Result<Vec<_>, _>>()?;

        let pipeline_output_configs =
            make_pipeline_output_configs(&parameters, &scaling_algorithms);
        let pipeline = pipeline::ResizingPipeline::new(device.clone(), pipeline_output_configs)?;

        Ok(Self {
            decoder,
            parser,
            reference_ctx,
            sorter,
            resizing_pipeline: pipeline,
            encoders,
            device,
        })
    }

    /// Transcodes the input bytes and returns a [`Vec`] where each element corresponds to an
    /// output frame. Each frame is a [`Vec`] where each element corresponds to one output.
    pub fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, TranscoderError> {
        let instructions = self.parse_input(input)?;
        self.transcode_instructions(instructions)
    }

    /// Flush the internal queues of the transcoder. Only do this once you're sure no new frames
    /// are coming, otherwise the output may have the wrong frame order. Returns a [`Vec`] where
    /// each element corresponds to an output frame. Each frame is a [`Vec`] where each element
    /// corresponds to one output.
    pub fn flush(&mut self) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, TranscoderError> {
        let instructions = self.flush_parser()?;
        let mut output = self.transcode_instructions(instructions)?;
        output.append(&mut self.flush_transcoder()?);

        Ok(output)
    }

    fn flush_parser(&mut self) -> Result<Vec<DecoderInstruction>, TranscoderError> {
        let access_units = self.parser.flush().map_err(DecoderError::from)?;
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)
            .map_err(DecoderError::from)?;

        Ok(instructions)
    }

    fn flush_transcoder(
        &mut self,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, TranscoderError> {
        let remaining = self.sorter.flush();

        let mut output = Vec::new();
        for resized_images in remaining {
            let encoded = self.encode_resized_images(resized_images)?;
            output.push(encoded);
        }

        Ok(output)
    }

    fn parse_input(
        &mut self,
        input: EncodedInputChunk<'_>,
    ) -> Result<Vec<DecoderInstruction>, TranscoderError> {
        let access_units = self
            .parser
            .parse(input.data, input.pts)
            .map_err(DecoderError::from)?;

        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)
            .map_err(DecoderError::from)?;

        Ok(instructions)
    }

    fn transcode_instructions(
        &mut self,
        instructions: Vec<DecoderInstruction>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, TranscoderError> {
        let mut encoded_frame_sets = Vec::new();

        for instruction in instructions {
            let Some(mut frame) = self
                .decoder
                .decode(&instruction)
                .map_err(DecoderError::from)?
            else {
                continue;
            };

            let mut trackers = self
                .encoders
                .iter_mut()
                .map(|e| &mut e.tracker)
                .collect::<Vec<_>>();
            let cropped_extent = frame.decode_result.frame.cropped_extent;
            let output = self
                .resizing_pipeline
                .run(&mut frame, &mut trackers, cropped_extent)?;

            let sorted = self.sorter.put(DecodeResult {
                frame: ResizedImages {
                    images: output,
                    decoder_wait_value: frame.semaphore_wait_value,
                    decode_query_pool: frame.decode_query_pool,
                    input_buffer: frame.input_buffer,
                    _in_flight_resources: frame.in_flight_resources,
                },
                metadata: frame.decode_result.metadata,
            });

            for resized_images in sorted {
                let encoded_frames = self.encode_resized_images(resized_images)?;
                encoded_frame_sets.push(encoded_frames);
            }
        }

        Ok(encoded_frame_sets)
    }

    fn encode_resized_images(
        &mut self,
        resized_images: OutputFrame<ResizedImages>,
    ) -> Result<Vec<EncodedOutputChunk<Vec<u8>>>, TranscoderError> {
        let mut submits = Vec::new();
        for (encoder, frame) in self
            .encoders
            .iter_mut()
            .zip(resized_images.data.images.outputs.iter())
        {
            let submit = encoder.encode(frame.image.clone(), false, resized_images.metadata.pts)?;
            submits.push(submit);
        }

        let mut semaphores = Vec::new();
        let mut values = Vec::new();
        for submit in submits.iter() {
            semaphores.push(
                submit
                    .0
                    .encoder
                    .tracker
                    .semaphore_tracker
                    .semaphore
                    .semaphore,
            );
            values.push(submit.0.wait_value.0);
        }
        let wait = vk::SemaphoreWaitInfo::default()
            .semaphores(&semaphores)
            .values(&values);
        unsafe { self.device.device.wait_semaphores(&wait, u64::MAX)? };

        let mut results = Vec::new();
        for submit in submits {
            let waited = submit.mark_waited();
            let result = waited.download()?;
            results.push(result);
        }

        // TODO: this is atrocious
        self.decoder
            .tracker
            .mark_waited(resized_images.data.decoder_wait_value);
        resized_images.data.input_buffer.release_to_pool();

        self.resizing_pipeline
            .mark_command_buffers_completed(resized_images.data.decoder_wait_value);
        self.resizing_pipeline
            .free_submission(resized_images.data.images);

        if let Some(query_pool) = resized_images.data.decode_query_pool {
            query_pool
                .check_results_blocking()
                .map_err(DecoderError::VulkanDecoderError)?;
        }

        Ok(results)
    }
}

fn make_pipeline_output_configs(
    parameters: &[FullEncoderParameters],
    scaling_algorithms: &[crate::parameters::ScalingAlgorithm],
) -> Vec<OutputConfig> {
    parameters
        .iter()
        .zip(scaling_algorithms.iter())
        .map(|(p, &scaling)| OutputConfig {
            width: p.width.get(),
            height: p.height.get(),
            profile: H264EncodeProfileInfo::new_encode(p),
            scaling_algorithm: scaling,
        })
        .collect()
}
