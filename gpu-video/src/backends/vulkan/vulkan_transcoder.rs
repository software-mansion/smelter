use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ash::vk;

use crate::{
    EncodedInputChunk, EncodedOutputChunk, H264ParserError, OutputFrame, ReferenceManagementError,
    TranscodedChunk, VideoBackendError, VideoTranscoderError,
    backends::vulkan::{
        AsyncVulkanEncoder, VulkanCommonError, VulkanDecoder, VulkanDecoderError, VulkanDevice,
        codec::{EncodeCodec, h264::H264Codec, h265::H265Codec},
        vulkan_decoder::{ImageModifiers, InFlightDecodeResources},
        vulkan_encoder::{
            FullEncoderParameters, VulkanEncoder, VulkanEncoderError,
            async_encoder::{DynVulkanEncoder, OnEncodedChunkCallback},
        },
        vulkan_transcoder::pipeline::{
            OutputConfig, ResizeSubmission, ResizingImageBundle, ResizingPipeline,
        },
        waiter_thread::{SubmissionWaitRequest, WaiterThreadHandle},
        wrappers::{CommandBufferPoolStorage, EncodeInputImage, ResultQuery, SemaphoreWaitValue},
    },
    frame_sorter::{DecodeResult, FrameSorter},
    parameters::DecoderUsage,
    parser::{
        decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
        h264::H264Parser,
        reference_manager::ReferenceContext,
    },
    transcoder::{AnyEncoderParameters, TranscoderParameters, VideoTranscoderBackend},
};

mod pipeline;

#[derive(Debug, Clone, Copy)]
enum AnyFullEncoderParameters {
    H264(FullEncoderParameters<H264Codec>),
    H265(FullEncoderParameters<H265Codec>),
}

pub(crate) struct ResizedImages {
    images: Box<[ResizingImageBundle<EncodeInputImage>]>,
    // TODO: make sure there's enough of queries in the pool
    // result_query: Option<ResultQuery<vk::QueryResultStatusKHR>>,
}

pub struct VulkanTranscoder {
    device: Arc<VulkanDevice>,
    decoder: VulkanDecoder<'static>,
    parser: H264Parser,
    reference_ctx: ReferenceContext,
    sorter: FrameSorter<ResizedImages>,
    resizing_pipeline: ResizingPipeline,
    waiter_thread: Arc<WaiterThreadHandle>,
    encoders: Vec<Box<dyn DynVulkanEncoder>>,
}

impl Drop for VulkanTranscoder {
    fn drop(&mut self) {
        for encoder in self.encoders.iter_mut() {
            encoder.wait_for_all(Duration::MAX).unwrap();
        }
    }
}

impl VideoTranscoderBackend for VulkanTranscoder {
    // TODO: handle timeout
    fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
        timeout: Duration,
    ) -> Result<(), VideoTranscoderError> {
        VulkanTranscoder::transcode(self, input).map_err(Into::into)
    }

    // TODO: handle timeout
    fn flush(&mut self, timeout: Duration) -> Result<(), VideoTranscoderError> {
        VulkanTranscoder::flush(self).map_err(Into::into)
    }
}

impl VulkanTranscoder {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        config: TranscoderParameters,
        waiter_thread: Arc<WaiterThreadHandle>,
        on_chunk_callback: Box<dyn FnMut(TranscodedChunk) + Send>,
    ) -> Result<Self, VulkanTranscoderError> {
        let decoder = VulkanDecoder::new(
            Arc::new(device.decoding_device()?),
            DecoderUsage::Transcoding,
            ImageModifiers {
                create_flags: vk::ImageCreateFlags::EXTENDED_USAGE
                    | vk::ImageCreateFlags::MUTABLE_FORMAT,
                usage_flags: vk::ImageUsageFlags::STORAGE,
                additional_queue_index: device.queues.compute.family_index,
            },
            // TODO: set it correctly
            // i'm sure it's fine
            64,
        )?;

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
            .map(|c| match c.encoder_parameters {
                AnyEncoderParameters::H264(params) => device
                    .validate_and_fill_encoder_parameters(
                        params,
                        c.output_width,
                        c.output_height,
                        config.input_framerate,
                        Some(3), // TODO: async transcoder
                    )
                    .map(AnyFullEncoderParameters::H264),

                AnyEncoderParameters::H265(params) => device
                    .validate_and_fill_encoder_parameters(
                        params,
                        c.output_width,
                        c.output_height,
                        config.input_framerate,
                        Some(3), // TODO: async transcoder
                    )
                    .map(AnyFullEncoderParameters::H265),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let encode_image_queue_indices = vec![device.queues.compute.family_index as u32];
        let on_chunk_callback = Arc::new(Mutex::new(on_chunk_callback));
        let encoders = parameters
            .iter()
            .copied()
            .enumerate()
            .map(|(output_index, p)| {
                let on_chunk_callback = on_chunk_callback.clone();
                let on_chunk_callback = Box::new(move |chunk| {
                    let mut callback = on_chunk_callback.lock().unwrap();
                    (callback)(TranscodedChunk {
                        output_index,
                        chunk,
                    })
                });

                match p {
                    AnyFullEncoderParameters::H264(p) => device
                        .encoding_device()
                        .and_then(|d| {
                            AsyncVulkanEncoder::new_with_input_images(
                                Arc::new(d),
                                p,
                                on_chunk_callback,
                                waiter_thread.clone(),
                                vk::ImageUsageFlags::STORAGE,
                                encode_image_queue_indices.clone(),
                            )
                        })
                        .map(|e| Box::new(e) as Box<dyn DynVulkanEncoder>),

                    AnyFullEncoderParameters::H265(p) => device
                        .encoding_device()
                        .and_then(|d| {
                            AsyncVulkanEncoder::new_with_input_images(
                                Arc::new(d),
                                p,
                                on_chunk_callback,
                                waiter_thread.clone(),
                                vk::ImageUsageFlags::STORAGE,
                                encode_image_queue_indices.clone(),
                            )
                        })
                        .map(|e| Box::new(e) as Box<dyn DynVulkanEncoder>),
                }
            })
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
            waiter_thread,
            device,
        })
    }

    /// Transcodes the input bytes and returns a [`Vec`] where each element corresponds to an
    /// output frame. Each frame is a [`Vec`] where each element corresponds to one output.
    pub fn transcode(&mut self, input: EncodedInputChunk<'_>) -> Result<(), VulkanTranscoderError> {
        let instructions = self.parse_input(input)?;
        self.transcode_instructions(instructions)
    }

    /// Flush the internal queues of the transcoder. Only do this once you're sure no new frames
    /// are coming, otherwise the output may have the wrong frame order. Returns a [`Vec`] where
    /// each element corresponds to an output frame. Each frame is a [`Vec`] where each element
    /// corresponds to one output.
    pub fn flush(&mut self) -> Result<(), VulkanTranscoderError> {
        let instructions = self.flush_parser()?;
        self.transcode_instructions(instructions)?;
        self.flush_transcoder()
    }

    fn flush_parser(&mut self) -> Result<Vec<DecoderInstruction>, VulkanTranscoderError> {
        let access_units = self.parser.flush()?;
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;

        Ok(instructions)
    }

    // TODO: wait for all to finish
    fn flush_transcoder(&mut self) -> Result<(), VulkanTranscoderError> {
        let remaining = self.sorter.flush();
        for resized_images in remaining {
            self.encode_resized_images(resized_images)?;
        }
        Ok(())
    }

    fn parse_input(
        &mut self,
        input: EncodedInputChunk<'_>,
    ) -> Result<Vec<DecoderInstruction>, VulkanTranscoderError> {
        let access_units = self.parser.parse(input.data, input.pts)?;
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;

        Ok(instructions)
    }

    fn transcode_instructions(
        &mut self,
        instructions: Vec<DecoderInstruction>,
    ) -> Result<(), VulkanTranscoderError> {
        for instruction in instructions {
            let decoder_semaphore = self.decoder.tracker.semaphore_tracker.semaphore.clone();
            let decoder_command_buffer_pools = self.decoder.tracker.command_buffer_pools.clone();
            let resizing_pipeline_command_buffer_pools = self.resizing_pipeline.buffer_pool.clone();

            let Some(mut frame) = self.decoder.decode(instruction)? else {
                continue;
            };

            let output_images = self
                .encoders
                .iter_mut()
                .map(|e| e.next_input_image())
                .collect::<Result<Vec<_>, _>>()?;
            let mut trackers = self
                .encoders
                .iter_mut()
                .map(|e| e.tracker())
                .collect::<Vec<_>>();
            let metadata = &frame.decode_result.metadata;
            let cropped_extent = vk::Extent2D {
                width: metadata.cropped_width,
                height: metadata.cropped_height,
            };
            let output = self.resizing_pipeline.run(
                &mut frame,
                &mut trackers,
                output_images,
                cropped_extent,
            )?;

            // TODO: handle max in flight
            self.waiter_thread.submit(SubmissionWaitRequest {
                semaphore: decoder_semaphore,
                wait_for: output.wait_value,
                on_finish: Box::new(move || {
                    decoder_command_buffer_pools.mark_submitted_as_free(output.wait_value);
                    resizing_pipeline_command_buffer_pools
                        .mark_submitted_as_free(output.wait_value);
                    output.descriptors.release_to_pool();

                    drop(output.input);
                    drop(frame.in_flight_resources);

                    // TODO: do something about the error
                    if let Some(query) = frame.result_query.take() {
                        let _ = query.check_results_blocking();
                    }
                }),
            })?;

            let sorted = self.sorter.put(DecodeResult {
                frame: ResizedImages {
                    // TODO: inline
                    images: output.outputs,
                },
                metadata: frame.decode_result.metadata,
            });

            for resized_images in sorted {
                self.encode_resized_images(resized_images)?;
            }
        }

        Ok(())
    }

    fn encode_resized_images(
        &mut self,
        resized_images: OutputFrame<ResizedImages>,
    ) -> Result<(), VulkanTranscoderError> {
        for (encoder, frame) in self
            .encoders
            .iter_mut()
            .zip(resized_images.data.images.into_iter())
        {
            // TODO: encode can block if > max_in_flight so other submits won't happen
            // TODO: views need to be kept alive
            encoder.encode(
                frame.image,
                Box::new([frame.view_y, frame.view_uv]),
                false,
                resized_images.metadata.pts,
            )?;
        }

        Ok(())
    }
}

fn make_pipeline_output_configs(
    parameters: &[AnyFullEncoderParameters],
    scaling_algorithms: &[crate::parameters::ScalingAlgorithm],
) -> Vec<OutputConfig> {
    parameters
        .iter()
        .zip(scaling_algorithms.iter())
        .map(|(p, &scaling)| match p {
            AnyFullEncoderParameters::H264(p) => OutputConfig {
                width: p.width.get(),
                height: p.height.get(),
                profile: H264Codec::profile_info(p),
                scaling_algorithm: scaling,
            },

            AnyFullEncoderParameters::H265(p) => OutputConfig {
                width: p.width.get(),
                height: p.height.get(),
                profile: H265Codec::profile_info(p),
                scaling_algorithm: scaling,
            },
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum VulkanTranscoderError {
    #[error(transparent)]
    Decoder(#[from] VulkanDecoderError),

    #[error(transparent)]
    Encoder(#[from] VulkanEncoderError),

    #[error("Reference management error: {0}")]
    ReferenceManagementError(#[from] ReferenceManagementError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] H264ParserError),

    #[error(transparent)]
    Common(#[from] VulkanCommonError),

    #[error("Vulkan error: {0}")]
    Vulkan(#[from] vk::Result),

    #[error("Wrong output number: expected a value between 0 and {expected_max}, found {actual}")]
    WrongOutputNumber { expected_max: usize, actual: usize },
}

impl From<VulkanTranscoderError> for VideoTranscoderError {
    fn from(err: VulkanTranscoderError) -> Self {
        match err {
            VulkanTranscoderError::Decoder(err) => VideoTranscoderError::Decoder(err.into()),
            VulkanTranscoderError::ReferenceManagementError(err) => {
                VideoTranscoderError::Decoder(err.into())
            }
            VulkanTranscoderError::ParserError(err) => VideoTranscoderError::Decoder(err.into()),
            VulkanTranscoderError::Encoder(err) => VideoTranscoderError::Encoder(err.into()),
            VulkanTranscoderError::WrongOutputNumber {
                expected_max,
                actual,
            } => VideoTranscoderError::WrongOutputNumber {
                expected_max,
                actual,
            },
            VulkanTranscoderError::Common(_) | VulkanTranscoderError::Vulkan(_) => {
                VideoTranscoderError::BackendError(VideoBackendError {
                    message: err.to_string(),
                    source: Box::new(err),
                })
            }
        }
    }
}
