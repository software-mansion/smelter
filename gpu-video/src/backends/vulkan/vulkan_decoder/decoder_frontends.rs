use std::{collections::VecDeque, sync::Arc};

use crate::{
    DecoderEvent, OutputFrame, RawFrameData,
    backends::vulkan::{
        VulkanDecoder,
        task_thread::TaskThread,
        vulkan_decoder::{
            DecodeSubmission, DecoderTracker, DownloadSubmission, ImageModifiers,
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
    frame_sorter: FrameSorter<DownloadSubmission<O::DecodedGpuFrame>>,

    max_in_flight_submissions: usize,
    in_flight: VecDeque<SemaphoreWaitValue>,

    output: O,
    task_thread: Arc<TaskThread>,
}

impl<O: DecodeOutput> VideoDecoderBackend for VulkanDecoderH264<O> {
    fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<(), VideoDecoderError> {
        match event {
            DecoderEvent::DecodeChunk(chunk) => {
                let access_units = self.parser.parse(chunk.data, chunk.pts)?;
                let frames = self.decode_access_units(access_units)?;
                self.send_to_output(frames);
            }
            DecoderEvent::DecodeParsedFrame(au) => {
                let frames = self.decode_access_units(vec![au])?;
                self.send_to_output(frames);
            }
            DecoderEvent::SignalFrameEnd => {
                let access_units = self.parser.flush()?;
                let frames = self.decode_access_units(access_units)?;
                self.send_to_output(frames);
            }
            DecoderEvent::SignalDataLoss => {
                self.reference_ctx.mark_missed_frames();
            }
            DecoderEvent::Flush => {
                let access_units = self.parser.flush()?;
                let mut frames = self.decode_access_units(access_units)?;
                frames.append(&mut self.frame_sorter.flush());

                self.send_to_output(frames);
                self.task_thread.sync();
                self.in_flight.clear();
            }
        }

        Ok(())
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
            max_in_flight_submissions: parameters.max_in_flight_submissions.get(),
            in_flight: VecDeque::new(),
            output,
            task_thread,
        })
    }

    fn decode_access_units(
        &mut self,
        access_units: Vec<AccessUnit>,
    ) -> Result<Vec<OutputFrame<DownloadSubmission<O::DecodedGpuFrame>>>, VideoDecoderError> {
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;
        let decoded = self.run_decode_instructions(instructions)?;
        let frames = self.frame_sorter.put_frames(decoded);

        Ok(frames)
    }

    pub(crate) fn run_decode_instructions(
        &mut self,
        decoder_instructions: Vec<DecoderInstruction>,
    ) -> Result<Vec<DecodeResult<DownloadSubmission<O::DecodedGpuFrame>>>, VulkanDecoderError> {
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

    fn send_to_output(&mut self, frames: Vec<OutputFrame<DownloadSubmission<O::DecodedGpuFrame>>>) {
        let output = self.output.clone();
        let tracker = self.decoder.tracker.clone();

        self.output.on_submit(&frames);
        self.task_thread
            .submit(move || wait_for_frames(&output, &tracker, frames));
    }
}

fn wait_for_frames<O: DecodeOutput>(
    output: &O,
    tracker: &DecoderTracker,
    frames: Vec<OutputFrame<DownloadSubmission<O::DecodedGpuFrame>>>,
) {
    let Some(max_wait_value) = frames.iter().map(|f| f.data.semaphore_wait_value).max() else {
        return;
    };

    if let Err(err) = tracker.wait_for(max_wait_value, u64::MAX) {
        output.on_error(VulkanDecoderError::VulkanCommonError(err).into());
        return;
    }

    output.on_finish(frames);
}

pub(crate) trait DecodeOutput: Clone + Send + 'static {
    /// Represents frame that's on GPU. Could be a buffer, wgpu::Texture, etc.
    type DecodedGpuFrame: Send + 'static;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadSubmission<Self::DecodedGpuFrame>, VulkanDecoderError>;

    /// Called just before the submissions are sent to the background thread.
    fn on_submit(&self, frames: &[OutputFrame<DownloadSubmission<Self::DecodedGpuFrame>>]);

    /// Called just after the submissions finish their work.
    fn on_finish(&self, frames: Vec<OutputFrame<DownloadSubmission<Self::DecodedGpuFrame>>>);

    /// Handles submission errors that happen in the background thread.
    fn on_error(&self, error: VideoDecoderError);
}

/// Used by BytesDecoder. Decoded frames are sent to the user via callback when the decode submission finishes.
#[derive(Clone)]
pub(crate) struct BytesOutput {
    on_frame_callback: FrameCallback<RawFrameData>,
}

impl BytesOutput {
    pub(crate) fn new(on_frame_callback: FrameCallback<RawFrameData>) -> Self {
        Self { on_frame_callback }
    }
}

impl DecodeOutput for BytesOutput {
    type DecodedGpuFrame = DecodeResult<Buffer>;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadSubmission<DecodeResult<Buffer>>, VulkanDecoderError> {
        let (buffer, semaphore_wait_value) = submission
            .decoder
            .download_output(&submission.decode_result)?;

        Ok(DownloadSubmission {
            frame: DecodeResult {
                frame: buffer,
                metadata: submission.decode_result.metadata,
            },
            semaphore_wait_value,
            _in_flight_resources: submission.in_flight_resources,
            decode_query_pool: submission.decode_query_pool,
        })
    }

    fn on_submit(&self, _frames: &[OutputFrame<DownloadSubmission<DecodeResult<Buffer>>>]) {
        // do nothing
    }

    fn on_finish(&self, frames: Vec<OutputFrame<DownloadSubmission<DecodeResult<Buffer>>>>) {
        let mut on_frame_callback = self.on_frame_callback.lock().unwrap();

        for frame in frames {
            let OutputFrame { mut data, metadata } = frame;

            let width = data.frame.metadata.cropped_width;
            let height = data.frame.metadata.cropped_height;

            let frame_download_result = unsafe {
                data.frame
                    .frame
                    .download_data_from_buffer(width as usize * height as usize * 3 / 2)
            }
            .map_err(VulkanDecoderError::from)
            .map_err(VideoDecoderError::from)
            .and_then(|frame| {
                data.check_decode_results()?;
                Ok(OutputFrame {
                    data: RawFrameData {
                        frame,
                        width,
                        height,
                    },
                    metadata,
                })
            });

            (on_frame_callback)(frame_download_result)
        }
    }

    fn on_error(&self, error: VideoDecoderError) {
        let mut on_frame_callback = self.on_frame_callback.lock().unwrap();
        (on_frame_callback)(Err(error))
    }
}

/// Used by WgpuTexturesDecoder. We don't wait for the submission to finish before sending it to the user,
/// because wgpu handles the synchronization.
#[cfg(feature = "wgpu")]
#[derive(Clone)]
pub(crate) struct WgpuTexturesOutput {
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,
    on_frame_callback: FrameCallback<wgpu::Texture>,
}

#[cfg(feature = "wgpu")]
impl WgpuTexturesOutput {
    pub(crate) fn new(
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        on_frame_callback: FrameCallback<wgpu::Texture>,
    ) -> Self {
        Self {
            wgpu_device,
            wgpu_queue,
            on_frame_callback,
        }
    }
}

#[cfg(feature = "wgpu")]
impl DecodeOutput for WgpuTexturesOutput {
    type DecodedGpuFrame = wgpu::Texture;

    fn start_download(
        &self,
        submission: DecodeSubmission<'_, '_>,
    ) -> Result<DownloadSubmission<wgpu::Texture>, VulkanDecoderError> {
        let (frame, semaphore_wait_value) = submission.decoder.output_to_wgpu_texture(
            &self.wgpu_device,
            &self.wgpu_queue,
            &submission.decode_result,
        )?;

        Ok(DownloadSubmission {
            frame,
            semaphore_wait_value,
            _in_flight_resources: submission.in_flight_resources,
            decode_query_pool: submission.decode_query_pool,
        })
    }

    /// Frames are sent to the user without waiting.
    fn on_submit(&self, frames: &[OutputFrame<DownloadSubmission<wgpu::Texture>>]) {
        let mut on_frame_callback = self.on_frame_callback.lock().unwrap();
        for OutputFrame { data, metadata } in frames {
            (on_frame_callback)(Ok(OutputFrame {
                data: data.frame.clone(),
                metadata: metadata.clone(),
            }))
        }
    }

    fn on_finish(&self, frames: Vec<OutputFrame<DownloadSubmission<wgpu::Texture>>>) {
        for frame in frames {
            if let Err(err) = frame.data.check_decode_results() {
                self.on_error(err.into());
            }
        }
    }

    fn on_error(&self, error: VideoDecoderError) {
        let mut on_frame_callback = self.on_frame_callback.lock().unwrap();
        (on_frame_callback)(Err(error))
    }
}
