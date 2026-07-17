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
        task_thread::TaskThread,
        vulkan_decoder::{
            DecodeSubmission, DecoderTracker, DownloadDecodeSubmission, ImageModifiers,
            VulkanDecoderError,
        },
        vulkan_device::DecodingDevice,
        wrappers::{Buffer, SemaphoreWaitValue},
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
    task_thread: Arc<TaskThread>,
}

impl VideoDecoderBackend for VulkanDecoderH264<BytesOutput> {
    fn process_event_bytes(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<(), VideoDecoderError> {
        let block_until_done = matches!(event, DecoderEvent::Flush);
        let frames = self.process_event(event)?;

        self.submit_to_task_thread(frames);

        if block_until_done {
            self.task_thread.sync();
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

        self.submit_to_task_thread(frames);

        Ok(output_textures)
    }
}

impl<O: DecodeOutput> VulkanDecoderH264<O> {
    pub(crate) fn new(
        decoding_device: Arc<DecodingDevice>,
        parameters: DecoderParameters,
        output: O,
        task_thread: Arc<TaskThread>,
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
            task_thread,
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

    fn submit_to_task_thread(
        &self,
        frames: Vec<OutputFrame<DownloadDecodeSubmission<O::DecodedGpuFrame>>>,
    ) {
        let output = self.output.clone();
        let tracker = self.decoder.tracker.clone();
        let corrupted_state_signal = self.corrupted_state_signal.clone();

        self.task_thread.submit(move || {
            if let Err(err) = wait_for_all_submissions(&tracker, &frames) {
                tracing::debug!("Failed to wait for decode submissions: {err}");
                corrupted_state_signal.store(true, Ordering::Relaxed);
                return;
            }
            if let Err(err) = output.handle_finished_submissions(frames) {
                tracing::debug!("Frame decoding failed: {err}");
                corrupted_state_signal.store(true, Ordering::Relaxed);
            }
        });
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
    /// Represents frame that's on GPU. Could be a buffer, wgpu::Texture, etc.
    type DecodedGpuFrame: Send + 'static;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadDecodeSubmission<Self::DecodedGpuFrame>, VulkanDecoderError>;

    fn handle_finished_submissions(
        &self,
        frames: Vec<OutputFrame<DownloadDecodeSubmission<Self::DecodedGpuFrame>>>,
    ) -> Result<(), VideoDecoderError>;
}

#[derive(Clone)]
pub(crate) struct BytesOutput {
    on_frame_callback: FrameCallback<RawFrameData>,
}

impl DecodeOutput for BytesOutput {
    type DecodedGpuFrame = DecodeResult<Buffer>;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadDecodeSubmission<Self::DecodedGpuFrame>, VulkanDecoderError> {
        let (buffer, semaphore_wait_value) = submission
            .decoder
            .output_to_buffer(&submission.decode_result)?;

        Ok(DownloadDecodeSubmission {
            frame: DecodeResult {
                frame: buffer,
                metadata: submission.decode_result.metadata,
            },
            semaphore_wait_value,
            _in_flight_resources: submission.in_flight_resources,
            decode_query_pool: submission.decode_query_pool,
        })
    }

    fn handle_finished_submissions(
        &self,
        frames: Vec<OutputFrame<DownloadDecodeSubmission<Self::DecodedGpuFrame>>>,
    ) -> Result<(), VideoDecoderError> {
        let mut on_frame_callback = self.on_frame_callback.lock().unwrap();
        for frame in frames {
            let frame = frame.download_bytes_frame()?;
            (on_frame_callback)(frame)
        }

        Ok(())
    }
}

impl BytesOutput {
    pub(crate) fn new(on_frame_callback: FrameCallback<RawFrameData>) -> Self {
        Self { on_frame_callback }
    }
}

#[cfg(feature = "wgpu")]
#[derive(Clone)]
pub(crate) struct WgpuTexturesOutput {
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,
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
            semaphore_wait_value,
            _in_flight_resources: submission.in_flight_resources,
            decode_query_pool: submission.decode_query_pool,
        })
    }

    fn handle_finished_submissions(
        &self,
        frames: Vec<OutputFrame<DownloadDecodeSubmission<Self::DecodedGpuFrame>>>,
    ) -> Result<(), VideoDecoderError> {
        for frame in frames {
            frame.data.check_decode_results()?;
        }

        Ok(())
    }
}

#[cfg(feature = "wgpu")]
impl WgpuTexturesOutput {
    pub(crate) fn new(wgpu_device: wgpu::Device, wgpu_queue: wgpu::Queue) -> Self {
        Self {
            wgpu_device,
            wgpu_queue,
        }
    }
}

fn wait_for_all_submissions<T>(
    tracker: &DecoderTracker,
    frames: &[OutputFrame<DownloadDecodeSubmission<T>>],
) -> Result<(), VideoDecoderError> {
    let Some(max_wait_value) = frames.iter().map(|f| f.data.semaphore_wait_value).max() else {
        return Ok(());
    };

    tracker
        .wait_for(max_wait_value, u64::MAX)
        .map_err(VulkanDecoderError::from)
        .map_err(VideoDecoderError::from)
}

impl OutputFrame<DownloadDecodeSubmission<DecodeResult<Buffer>>> {
    fn download_bytes_frame(self) -> Result<OutputFrame<RawFrameData>, VulkanDecoderError> {
        let OutputFrame { mut data, metadata } = self;

        let width = data.frame.metadata.cropped_width;
        let height = data.frame.metadata.cropped_height;

        let frame = unsafe {
            data.frame
                .frame
                .download_data_from_buffer(width as usize * height as usize * 3 / 2)?
        };
        data.check_decode_results()?;

        Ok(OutputFrame {
            data: RawFrameData {
                frame,
                width,
                height,
            },
            metadata,
        })
    }
}
