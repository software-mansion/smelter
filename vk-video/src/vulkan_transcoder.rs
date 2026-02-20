use std::sync::Arc;

use ash::vk;

use crate::{
    DecoderError, EncodedInputChunk, EncodedOutputChunk, VulkanCommonError, VulkanDevice,
    VulkanEncoderError,
    device::EncoderParameters,
    parser::{
        decoder_instructions::compile_to_decoder_instructions, h264::H264Parser,
        reference_manager::ReferenceContext,
    },
    vulkan_decoder::{DecodeResult, FrameSorter, ImageModifiers, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
    vulkan_transcoder::pipeline::{OutputConfig, Pipeline, ResizeSubmission, ResizingImageBundle},
    wrappers::SemaphoreWaitValue,
};

mod pipeline;

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

pub struct ResizedImages {
    images: ResizeSubmission,
    decoder_wait_value: SemaphoreWaitValue,
}

pub struct Transcoder {
    device: Arc<VulkanDevice>,
    decoder: VulkanDecoder<'static>,
    parser: H264Parser,
    reference_ctx: ReferenceContext,
    sorter: FrameSorter<ResizedImages>,
    pipeline: Pipeline,
    encoders: Vec<VulkanEncoder<'static>>,
    parameters: Vec<EncoderParameters>,
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

        let mut encoders = parameters
            .iter()
            .map(|p| {
                let parameters = device.validate_and_fill_encoder_parameters(p.clone());
                parameters.and_then(|p| VulkanEncoder::new(Arc::new(device.encoding_device()?), p))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let output_configs = output_configs(&mut encoders, &parameters);

        let pipeline = pipeline::Pipeline::new(device.clone(), &output_configs)?;

        Ok(Self {
            decoder,
            parser,
            reference_ctx,
            parameters,
            sorter,
            pipeline,
            encoders,
            device,
        })
    }

    pub fn transcode(
        &mut self,
        input: EncodedInputChunk<&[u8]>,
    ) -> Result<Vec<Vec<EncodedOutputChunk<Vec<u8>>>>, TranscoderError> {
        let access_units = self
            .parser
            .parse(input.data, input.pts)
            .map_err(DecoderError::from)?;

        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)
            .map_err(DecoderError::from)?;

        let mut encoded_frames = Vec::new();

        for instruction in instructions {
            let Some(frame) = self
                .decoder
                .decode(&instruction)
                .map_err(DecoderError::from)?
            else {
                continue;
            };

            let output = self.pipeline.run(
                &frame,
                &mut self.decoder.tracker,
                &mut output_configs(&mut self.encoders, &self.parameters),
            )?;

            let sorted = self.sorter.put(DecodeResult {
                frame: ResizedImages {
                    images: output,
                    decoder_wait_value: frame.semaphore_wait_value,
                },
                pts: frame.pts,
                pic_order_cnt: frame.picture_order_cnt,
                max_num_reorder_frames: frame.max_num_reorder_frames,
                is_idr: frame.is_idr,
            });

            for resized_images in sorted {
                let mut submits = Vec::new();
                for (encoder, frame) in self.encoders.iter_mut().zip(resized_images.data.images.outputs) {
                    let submit = encoder.encode(frame.image, false)?;
                    submits.push(submit);
                }

                let mut semaphores = Vec::new();
                let mut values = Vec::new();
                for submit in submits.iter() {
                    semaphores.push(submit.encoder.tracker.semaphore_tracker.semaphore.semaphore);
                    values.push(submit.wait_value.0);
                }

                let wait = vk::SemaphoreWaitInfo::default()
                    .semaphores(&semaphores)
                    .values(&values);
                unsafe { self.device.device.wait_semaphores(&wait, u64::MAX)? };

                let mut results = Vec::new();
                for mut submit in submits {
                    let is_keyframe = submit.is_idr;
                    submit.mark_waited();
                    let result = submit.download()?;
                    results.push(EncodedOutputChunk {
                        data: result,
                        pts: resized_images.pts,
                        is_keyframe,
                    });
                }

                self.decoder
                    .tracker
                    .mark_waited(resized_images.data.decoder_wait_value);
                self.pipeline.mark_command_buffers_completed();
                self.pipeline.free_descriptors(resized_images.data.images.descriptors);
                encoded_frames.push(results);
            }
        }

        Ok(encoded_frames)
    }
}

fn output_configs<'a: 'c, 'b: 'c, 'c>(
    encoders: &'a mut [VulkanEncoder<'b>],
    parameters: &[EncoderParameters],
) -> Vec<OutputConfig<'c>> {
    encoders
        .iter_mut()
        .zip(parameters.iter())
        .map(|(e, p)| OutputConfig {
            width: p.video_parameters.width.get(),
            height: p.video_parameters.height.get(),
            tracker: &mut e.tracker,
            profile: &e.profile_info,
        })
        .collect()
}
