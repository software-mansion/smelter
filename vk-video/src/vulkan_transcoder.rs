use std::sync::Arc;

use ash::vk;

use crate::{
    DecoderError, EncodedInputChunk, EncodedOutputChunk, Frame, VulkanCommonError, VulkanDevice,
    VulkanEncoderError,
    device::EncoderParameters,
    parser::{
        decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
        h264::H264Parser,
        reference_manager::ReferenceContext,
    },
    vulkan_decoder::{DecodeResult, FrameSorter, ImageModifiers, VulkanDecoder},
    vulkan_encoder::{FullEncoderParameters, H264EncodeProfileInfo, VulkanEncoder},
    vulkan_transcoder::pipeline::{OutputConfig, ResizeSubmission, ResizingPipeline},
    wrappers::{DecodeInputBuffer, SemaphoreWaitValue},
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

pub(crate) struct ResizedImages {
    images: ResizeSubmission,
    decoder_wait_value: SemaphoreWaitValue,
    input_buffer: DecodeInputBuffer,
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
        parameters: Vec<EncoderParameters>,
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
                additional_queue_index: device.queues.wgpu.family_index,
            },
        )
        .map_err(DecoderError::VulkanDecoderError)?;

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::default();
        let sorter = FrameSorter::new();

        let parameters = parameters
            .iter()
            .copied()
            .map(|p| device.validate_and_fill_encoder_parameters(p))
            .collect::<Result<Vec<_>, _>>()?;

        let encoders = parameters
            .iter()
            .copied()
            .map(|p| VulkanEncoder::new(Arc::new(device.encoding_device()?), p))
            .collect::<Result<Vec<_>, _>>()?;

        let output_configs = output_configs(&parameters);
        let pipeline = pipeline::ResizingPipeline::new(device.clone(), output_configs)?;

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
        input: EncodedInputChunk<&[u8]>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, TranscoderError> {
        let instructions = self.parse_input(input)?;
        self.transcode_instructions(instructions)
    }

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
        input: EncodedInputChunk<&[u8]>,
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
            let output = self.resizing_pipeline.run(&mut frame, &mut trackers)?;

            let sorted = self.sorter.put(DecodeResult {
                frame: ResizedImages {
                    images: output,
                    decoder_wait_value: frame.semaphore_wait_value,
                    input_buffer: frame.input_buffer,
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
        resized_images: Frame<ResizedImages>,
    ) -> Result<Vec<EncodedOutputChunk<Vec<u8>>>, TranscoderError> {
        let mut submits = Vec::new();
        for (encoder, frame) in self
            .encoders
            .iter_mut()
            .zip(resized_images.data.images.outputs.iter())
        {
            let submit = encoder.encode(frame.image.clone(), false, resized_images.pts)?;
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
        self.decoder
            .free_input_buffer(resized_images.data.input_buffer);

        self.resizing_pipeline
            .mark_command_buffers_completed(resized_images.data.decoder_wait_value);
        self.resizing_pipeline
            .free_submission(resized_images.data.images);

        Ok(results)
    }
}

fn output_configs(parameters: &[FullEncoderParameters]) -> Vec<OutputConfig> {
    parameters
        .iter()
        .map(|p| OutputConfig {
            width: p.width.get(),
            height: p.height.get(),
            profile: H264EncodeProfileInfo::new_encode(p),
        })
        .collect()
}
