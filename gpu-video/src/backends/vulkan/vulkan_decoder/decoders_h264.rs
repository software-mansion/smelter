use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    DecoderEvent, OutputFrame, RawFrameData,
    backends::vulkan::{
        VulkanDecoder,
        vulkan_decoder::{
            DecodeSubmission, DownloadDecodeSubmission, ImageModifiers, VulkanDecoderError,
        },
        vulkan_device::DecodingDevice,
        waiter_thread::{SubmissionWaitRequest, WaiterThreadHandle},
        wrappers::{Buffer, CommandBufferPoolStorage, SemaphoreWaitValue},
    },
    decoders::{FrameCallback, VideoDecoderBackend, VideoDecoderError},
    device::DecoderParameters,
    frame_sorter::{DecodeResult, FrameSorter},
    parser::{
        decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
        h264::{AccessUnit, H264Parser},
        reference_manager::ReferenceContext,
    },
};

pub(crate) struct VulkanDecoderH264<O: DecodeOutput> {
    decoder: VulkanDecoder<'static>,

    parser: H264Parser,
    reference_ctx: ReferenceContext,
    frame_sorter: FrameSorter<DownloadDecodeSubmission<O::DecodedGpuFrame>>,

    corrupted_state_signal: Arc<AtomicBool>,

    max_in_flight_submissions: usize,
    in_flight: VecDeque<SemaphoreWaitValue>,

    output: O,
    waiter_thread: Arc<WaiterThreadHandle>,
}

impl VideoDecoderBackend for VulkanDecoderH264<BytesOutput> {
    fn process_event_bytes(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<(), VideoDecoderError> {
        let block_until_done = matches!(event, DecoderEvent::Flush);
        let frames = self.process_event(event)?;

        self.submit_to_waiter_thread(frames)?;

        if block_until_done {
            self.waiter_thread
                .wait_for_semaphore(self.decoder.tracker.semaphore_tracker.semaphore.clone())
                .map_err(VulkanDecoderError::from)?;
            self.in_flight.clear();
        }

        Ok(())
    }
}

#[cfg(feature = "wgpu")]
impl crate::decoders::WgpuVideoDecoderBackend for VulkanDecoderH264<WgpuTexturesOutput> {
    fn process_event_textures(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VideoDecoderError> {
        let frames = self.process_event(event)?;
        let output_textures = frames
            .iter()
            .map(|f| {
                let OutputFrame { data, metadata } = f;
                OutputFrame {
                    data: data.frame.clone(),
                    metadata: metadata.clone(),
                }
            })
            .collect();

        self.submit_to_waiter_thread(frames)?;

        Ok(output_textures)
    }
}

impl<O: DecodeOutput> VulkanDecoderH264<O> {
    pub(crate) fn new(
        decoding_device: Arc<DecodingDevice>,
        parameters: DecoderParameters,
        output: O,
        waiter_thread: Arc<WaiterThreadHandle>,
    ) -> Result<Self, VulkanDecoderError> {
        let transfer_queue_idx = decoding_device.queues.transfer.family_index;
        let decoder = VulkanDecoder::new(
            decoding_device,
            parameters.usage_flags,
            ImageModifiers {
                additional_queue_index: transfer_queue_idx,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;

        Ok(Self {
            decoder,
            parser: H264Parser::default(),
            reference_ctx: ReferenceContext::new(parameters.missed_frame_handling),
            frame_sorter: FrameSorter::new(),
            max_in_flight_submissions: parameters.max_in_flight_submissions.get() as usize,
            in_flight: VecDeque::new(),
            output,
            waiter_thread,
            corrupted_state_signal: Arc::new(AtomicBool::new(false)),
        })
    }

    fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<OutputFrame<DownloadDecodeSubmission<O::DecodedGpuFrame>>>, VideoDecoderError>
    {
        if self.corrupted_state_signal.swap(false, Ordering::Relaxed) {
            self.reference_ctx.mark_corrupted_state();
        }

        match event {
            DecoderEvent::DecodeChunk(chunk) => {
                let access_units = self.parser.parse(chunk.data, chunk.pts)?;
                self.decode_access_units(access_units)
            }
            DecoderEvent::DecodeParsedFrame(au) => self.decode_access_units(vec![au]),
            DecoderEvent::SignalFrameEnd => {
                let access_units = self.parser.flush()?;
                self.decode_access_units(access_units)
            }
            DecoderEvent::SignalDataLoss => {
                self.reference_ctx.mark_corrupted_state();
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
    ) -> Result<Vec<OutputFrame<DownloadDecodeSubmission<O::DecodedGpuFrame>>>, VideoDecoderError>
    {
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;
        let decoded = self.run_decode_instructions(instructions)?;
        let frames = self.frame_sorter.put_frames(decoded);

        Ok(frames)
    }

    pub(crate) fn run_decode_instructions(
        &mut self,
        decoder_instructions: Vec<DecoderInstruction>,
    ) -> Result<Vec<DecodeResult<DownloadDecodeSubmission<O::DecodedGpuFrame>>>, VulkanDecoderError>
    {
        let mut frames = Vec::new();
        for instruction in decoder_instructions {
            if let Some(submission) = self.decoder.decode(instruction)? {
                let metadata = submission.decode_result.metadata.clone();
                let frame = self.output.start_download(submission)?;

                self.in_flight.push_back(frame.semaphore_wait_value);
                frames.push(DecodeResult { frame, metadata });
                self.throttle_submissions()?;
            }
        }

        Ok(frames)
    }

    fn submit_to_waiter_thread(
        &self,
        frames: Vec<OutputFrame<DownloadDecodeSubmission<O::DecodedGpuFrame>>>,
    ) -> Result<(), VulkanDecoderError> {
        let wait_requests = frames
            .into_iter()
            .map(|frame| {
                let output = self.output.clone();
                let corrupted_state_signal = self.corrupted_state_signal.clone();
                let command_buffer_pools = self.decoder.tracker.command_buffer_pools.clone();
                let wait_for = frame.data.semaphore_wait_value;

                SubmissionWaitRequest {
                    semaphore: self.decoder.tracker.semaphore_tracker.semaphore.clone(),
                    wait_for,
                    on_finish: Box::new(move || {
                        command_buffer_pools.mark_submitted_as_free(wait_for);
                        if let Err(err) = output.clone().handle_finished_frame(frame) {
                            tracing::debug!("Frame decoding failed: {err}");
                            corrupted_state_signal.store(true, Ordering::Relaxed);
                        }
                    }),
                }
            })
            .collect();

        Ok(self.waiter_thread.submit(wait_requests)?)
    }

    fn throttle_submissions(&mut self) -> Result<(), VulkanDecoderError> {
        let value = self.decoder.tracker.last_signaled_value()?;
        while self.in_flight.front().is_some_and(|v| *v <= value) {
            self.in_flight.pop_front();
        }

        while self.in_flight.len() > self.max_in_flight_submissions {
            let oldest_value = self.in_flight.pop_front().unwrap();
            self.decoder.tracker.wait_for(oldest_value, u64::MAX)?;
        }

        Ok(())
    }
}

pub(crate) trait DecodeOutput: Clone + Send + 'static {
    /// Represents frame that's on GPU (buffer or wgpu::Texture)
    type DecodedGpuFrame: Send + 'static;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadDecodeSubmission<Self::DecodedGpuFrame>, VulkanDecoderError>;

    fn handle_finished_frame(
        &self,
        frame: OutputFrame<DownloadDecodeSubmission<Self::DecodedGpuFrame>>,
    ) -> Result<(), VulkanDecoderError>;
}

#[derive(Clone)]
pub(crate) struct BytesOutput {
    pub(crate) on_frame_callback: FrameCallback<RawFrameData>,
}

impl DecodeOutput for BytesOutput {
    type DecodedGpuFrame = Buffer;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadDecodeSubmission<Self::DecodedGpuFrame>, VulkanDecoderError> {
        let (buffer, semaphore_wait_value) = submission
            .decoder
            .output_to_buffer(&submission.decode_result)?;

        Ok(DownloadDecodeSubmission {
            frame: buffer,
            decode_metadata: submission.decode_result.metadata,
            semaphore_wait_value,
            _in_flight_resources: submission.in_flight_resources,
            decode_query_pool: submission.decode_query_pool,
        })
    }

    fn handle_finished_frame(
        &self,
        frame: OutputFrame<DownloadDecodeSubmission<Self::DecodedGpuFrame>>,
    ) -> Result<(), VulkanDecoderError> {
        let OutputFrame { mut data, metadata } = frame;

        let width = data.decode_metadata.cropped_width;
        let height = data.decode_metadata.cropped_height;

        let frame = unsafe {
            data.frame
                .download_data_from_buffer(width as usize * height as usize * 3 / 2)?
        };
        data.check_decode_results()?;

        let mut on_frame_callback = self.on_frame_callback.lock().unwrap();
        (on_frame_callback)(OutputFrame {
            data: RawFrameData {
                frame,
                width,
                height,
            },
            metadata,
        });

        Ok(())
    }
}

#[cfg(feature = "wgpu")]
#[derive(Clone)]
pub(crate) struct WgpuTexturesOutput {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
}

#[cfg(feature = "wgpu")]
impl DecodeOutput for WgpuTexturesOutput {
    type DecodedGpuFrame = wgpu::Texture;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadDecodeSubmission<wgpu::Texture>, VulkanDecoderError> {
        let (frame, semaphore_wait_value) = submission.decoder.output_to_wgpu_texture(
            &self.wgpu_device,
            &self.wgpu_queue,
            &submission.decode_result,
        )?;

        Ok(DownloadDecodeSubmission {
            frame,
            decode_metadata: submission.decode_result.metadata,
            semaphore_wait_value,
            _in_flight_resources: submission.in_flight_resources,
            decode_query_pool: submission.decode_query_pool,
        })
    }

    fn handle_finished_frame(
        &self,
        frame: OutputFrame<DownloadDecodeSubmission<Self::DecodedGpuFrame>>,
    ) -> Result<(), VulkanDecoderError> {
        frame.data.check_decode_results()?;
        Ok(())
    }
}
