use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};

use ash::vk;
use tracing::error;

use crate::{
    DecoderEvent, EncodedInputChunk, H264ParserError, OutputFrame, ReferenceManagementError,
    TranscodedChunk, VideoBackendError, VideoTranscoderError,
    backends::vulkan::{
        AsyncVulkanEncoder, VulkanCommonError, VulkanDecoderError, VulkanDevice,
        codec::{h264::H264Codec, h265::H265Codec},
        vulkan_decoder::{ImageModifiers, decoders_h264::VulkanDecoderH264},
        vulkan_encoder::{
            FullEncoderParameters, VulkanEncoderError, async_encoder::DynVulkanEncoder,
        },
        vulkan_transcoder::pipeline::{OutputConfig, ResizingPipeline},
        waiter_thread::{SubmissionTracker, WaiterThreadHandle},
        wrappers::{CommandBufferPoolStorage, EncodeInputImage},
    },
    device::DecoderParameters,
    frame_sorter::{DecodeResult, FrameSorter},
    parameters::DecoderUsage,
    parser::decoder_instructions::DecoderInstruction,
    transcoder::{AnyEncoderParameters, TranscoderParameters, VideoTranscoderBackend},
};

mod pipeline;

#[derive(Debug, Clone, Copy)]
enum AnyFullEncoderParameters {
    H264(FullEncoderParameters<H264Codec>),
    H265(FullEncoderParameters<H265Codec>),
}

pub struct VulkanTranscoder {
    decoder: VulkanDecoderH264,
    sorter: FrameSorter<Box<[EncodeInputImage]>>,
    resizing_pipeline: ResizingPipeline,
    submission_tracker: SubmissionTracker,
    encoders: Vec<Box<dyn DynVulkanEncoder>>,
}

impl VideoTranscoderBackend for VulkanTranscoder {
    fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
        timeout: Duration,
    ) -> Result<(), VideoTranscoderError> {
        VulkanTranscoder::transcode(self, input, timeout).map_err(Into::into)
    }

    fn flush(&mut self, timeout: Duration) -> Result<(), VideoTranscoderError> {
        VulkanTranscoder::flush(self, timeout).map_err(Into::into)
    }
}

impl VulkanTranscoder {
    // TODO; make sure max_in_flight set to 0 works
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        config: TranscoderParameters,
        waiter_thread: Arc<WaiterThreadHandle>,
        on_chunk_callback: Box<dyn FnMut(TranscodedChunk) + Send>,
    ) -> Result<Self, VulkanTranscoderError> {
        let max_in_flight = config.max_in_flight_submissions.unwrap_or(3);

        let decoder = VulkanDecoderH264::new(
            Arc::new(device.decoding_device()?),
            DecoderParameters {
                corrupted_state_handling: Default::default(),
                usage_flags: DecoderUsage::Transcoding,
                max_in_flight_submissions: max_in_flight,
            },
            ImageModifiers {
                create_flags: vk::ImageCreateFlags::EXTENDED_USAGE
                    | vk::ImageCreateFlags::MUTABLE_FORMAT,
                usage_flags: vk::ImageUsageFlags::STORAGE,
                additional_queue_index: device.queues.compute.family_index,
            },
        )?;

        let sorter = FrameSorter::new();

        let scaling_algorithms: Vec<_> = config
            .output_parameters
            .iter()
            .map(|c| c.scaling_algorithm)
            .collect();

        let pipeline = pipeline::ResizingPipeline::new(
            device.clone(),
            scaling_algorithms
                .into_iter()
                .map(|scaling_algorithm| OutputConfig { scaling_algorithm })
                .collect(),
            max_in_flight,
        )?;

        let submission_tracker = SubmissionTracker::new(
            decoder.tracker().semaphore_tracker.semaphore.clone(),
            waiter_thread.clone(),
            max_in_flight as usize,
        );

        let encoder_params = encoder_params_from_config(&config, &device)?;
        let encoders =
            encoders_from_params(encoder_params, &device, waiter_thread, on_chunk_callback)?;

        Ok(Self {
            decoder,
            sorter,
            resizing_pipeline: pipeline,
            encoders,
            submission_tracker,
        })
    }

    // TODO: does timeout make sense here?
    // If one encode times out, the rest won't be scheduled
    // Another issue I forgot about:
    // If decoder parses something and then times out the parser still contains the bytes and the
    // state might be invalid
    pub fn transcode(
        &mut self,
        input: EncodedInputChunk<'_>,
        timeout: Duration,
    ) -> Result<(), VulkanTranscoderError> {
        let instructions = self
            .decoder
            .process_event(DecoderEvent::DecodeChunk(input))?;
        self.transcode_instructions(instructions, timeout)
    }

    pub fn flush(&mut self, timeout: Duration) -> Result<(), VulkanTranscoderError> {
        let instructions = self
            .decoder
            .process_event(DecoderEvent::Flush)
            .map_err(VulkanTranscoderError::from)?;
        self.transcode_instructions(instructions, timeout)?;
        self.flush_transcoder(timeout)
    }

    fn flush_transcoder(&mut self, timeout: Duration) -> Result<(), VulkanTranscoderError> {
        let remaining = self.sorter.flush();
        for resized_images in remaining {
            self.encode_resized_images(resized_images, timeout)?;
        }

        self.submission_tracker.wait_for_all(timeout)?;
        for encoder in self.encoders.iter_mut() {
            encoder.wait_for_all(timeout)?;
        }

        Ok(())
    }

    fn transcode_instructions(
        &mut self,
        instructions: Vec<DecoderInstruction>,
        timeout: Duration,
    ) -> Result<(), VulkanTranscoderError> {
        for instruction in instructions {
            let decoder_command_buffer_pools = self.decoder.tracker().command_buffer_pools.clone();
            let resizing_pipeline_command_buffer_pools = self.resizing_pipeline.buffer_pool.clone();
            let decode_failed_flag = self.decoder.decode_failed_flag();

            self.submission_tracker.wait_if_full(timeout)?;
            let Some(mut frame) = self.decoder.decode(instruction)? else {
                continue;
            };

            let metadata = &frame.decode_result.metadata;
            let cropped_extent = vk::Extent2D {
                width: metadata.cropped_width,
                height: metadata.cropped_height,
            };
            let output =
                self.resizing_pipeline
                    .run(&mut frame, cropped_extent, &mut self.encoders)?;

            self.submission_tracker
                .add_wait_request(output.wait_value, move || {
                    if let Some(query) = frame.result_query.take()
                        && let Err(err) = query.check_results_blocking()
                    {
                        error!("Decoding a frame failed: {err}");
                        decode_failed_flag.store(true, Ordering::Relaxed);
                    }

                    decoder_command_buffer_pools.mark_submitted_as_free(output.wait_value);
                    resizing_pipeline_command_buffer_pools
                        .mark_submitted_as_free(output.wait_value);

                    drop(frame.in_flight_resources);
                    drop(output.in_flight_resources);
                })?;

            let sorted = self.sorter.put(DecodeResult {
                frame: output.outputs,
                metadata: frame.decode_result.metadata,
            });

            for resized_images in sorted {
                self.encode_resized_images(resized_images, timeout)?;
            }
        }

        Ok(())
    }

    fn encode_resized_images(
        &mut self,
        resized_images: OutputFrame<Box<[EncodeInputImage]>>,
        timeout: Duration,
    ) -> Result<(), VulkanTranscoderError> {
        for encoder in self.encoders.iter_mut() {
            encoder.wait_if_full(timeout)?;
        }

        for (encoder, image) in self.encoders.iter_mut().zip(resized_images.data) {
            encoder.encode(image, false, resized_images.metadata.pts)?;
        }

        Ok(())
    }
}

fn encoder_params_from_config(
    config: &TranscoderParameters,
    device: &Arc<VulkanDevice>,
) -> Result<Vec<AnyFullEncoderParameters>, VulkanTranscoderError> {
    config
        .output_parameters
        .iter()
        .map(|c| match c.encoder_parameters {
            AnyEncoderParameters::H264(params) => device
                .validate_and_fill_encoder_parameters(
                    params,
                    c.output_width,
                    c.output_height,
                    config.input_framerate,
                    config.max_in_flight_submissions,
                )
                .map(AnyFullEncoderParameters::H264),

            AnyEncoderParameters::H265(params) => device
                .validate_and_fill_encoder_parameters(
                    params,
                    c.output_width,
                    c.output_height,
                    config.input_framerate,
                    config.max_in_flight_submissions,
                )
                .map(AnyFullEncoderParameters::H265),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(VulkanTranscoderError::from)
}

fn encoders_from_params(
    params: Vec<AnyFullEncoderParameters>,
    device: &Arc<VulkanDevice>,
    waiter_thread: Arc<WaiterThreadHandle>,
    on_chunk_callback: Box<dyn FnMut(TranscodedChunk) + Send>,
) -> Result<Vec<Box<dyn DynVulkanEncoder>>, VulkanTranscoderError> {
    let create_output_callback = {
        let on_chunk_callback = Arc::new(Mutex::new(on_chunk_callback));
        move |output_index| {
            let on_chunk_callback = on_chunk_callback.clone();
            Box::new(move |chunk| {
                let mut callback = on_chunk_callback.lock().unwrap();
                (callback)(TranscodedChunk {
                    output_index,
                    chunk,
                })
            })
        }
    };

    let encode_image_queue_indices = vec![device.queues.compute.family_index as u32];
    let encoders = params
        .iter()
        .copied()
        .enumerate()
        .map(|(output_index, p)| match p {
            AnyFullEncoderParameters::H264(p) => device
                .encoding_device()
                .and_then(|d| {
                    AsyncVulkanEncoder::new_with_input_images(
                        Arc::new(d),
                        p,
                        create_output_callback(output_index),
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
                        create_output_callback(output_index),
                        waiter_thread.clone(),
                        vk::ImageUsageFlags::STORAGE,
                        encode_image_queue_indices.clone(),
                    )
                })
                .map(|e| Box::new(e) as Box<dyn DynVulkanEncoder>),
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(encoders)
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
