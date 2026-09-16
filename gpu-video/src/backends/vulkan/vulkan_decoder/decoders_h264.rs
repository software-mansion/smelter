use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(feature = "wgpu")]
use crate::VideoTexture;
use crate::{
    DecoderEvent, OutputFrame, RawFrameData,
    backends::vulkan::{
        VulkanDecoder,
        vulkan_decoder::{
            DecodeSubmission, DecoderTracker, DownloadFrameSubmission, ImageModifiers,
            VulkanDecoderError,
        },
        vulkan_device::DecodingDevice,
        waiter_thread::{SubmissionTracker, WaiterThreadHandle},
        wrappers::{Buffer, CommandBufferPoolStorage},
    },
    decoders::{H264EventProcessor, VideoDecoderBackend, VideoDecoderError},
    device::DecoderParameters,
    frame_sorter::{DecodeResult, FrameSorter},
    parser::{
        decoder_instructions::DecoderInstruction,
        h264::{AccessUnit, H264Parser},
    },
};

pub(crate) struct VulkanDecoderH264 {
    decoder: VulkanDecoder<'static>,
    event_processor: H264EventProcessor,
}

impl VulkanDecoderH264 {
    pub(crate) fn new(
        decoding_device: Arc<DecodingDevice>,
        parameters: DecoderParameters,
        image_modifiers: ImageModifiers,
    ) -> Result<Self, VulkanDecoderError> {
        let decoder = VulkanDecoder::new(
            decoding_device,
            parameters.usage_flags,
            image_modifiers,
            parameters.max_in_flight_submissions.max(1),
        )?;

        Ok(Self {
            decoder,
            event_processor: H264EventProcessor::new(
                H264Parser::default(),
                parameters.corrupted_state_handling,
            ),
        })
    }

    pub(crate) fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<DecoderInstruction>, VideoDecoderError> {
        self.event_processor.process_event(event)
    }

    pub(crate) fn decode(
        &mut self,
        instruction: DecoderInstruction,
    ) -> Result<Option<DecodeSubmission<'_, 'static>>, VulkanDecoderError> {
        self.decoder.decode(instruction)
    }

    #[cfg_attr(not(feature = "transcoder"), expect(dead_code))]
    pub(crate) fn tracker(&self) -> &DecoderTracker {
        &self.decoder.tracker
    }

    #[cfg_attr(not(feature = "transcoder"), expect(dead_code))]
    pub(crate) fn decode_failed_flag(&self) -> Arc<AtomicBool> {
        self.event_processor.decode_failed_flag()
    }
}

pub(crate) struct VulkanBytesDecoderH264 {
    decoder: VulkanDecoderH264,
    submission_tracker: SubmissionTracker,
    output: Arc<Mutex<BytesOutput>>,
}

struct BytesOutput {
    frame_sorter: FrameSorter<RawFrameData>,
    on_frame_callback: Box<dyn FnMut(OutputFrame<RawFrameData>) + Send>,
}

impl VulkanBytesDecoderH264 {
    pub(crate) fn new(
        decoding_device: Arc<DecodingDevice>,
        parameters: DecoderParameters,
        on_frame_callback: Box<dyn FnMut(OutputFrame<RawFrameData>) + Send>,
        waiter_thread: Arc<WaiterThreadHandle>,
    ) -> Result<Self, VulkanDecoderError> {
        let transfer_queue_idx = decoding_device.queues.transfer.family_index;
        let decoder = VulkanDecoderH264::new(
            decoding_device,
            parameters,
            ImageModifiers {
                additional_queue_index: transfer_queue_idx,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;
        let submission_tracker = SubmissionTracker::new(
            decoder.decoder.tracker.semaphore_tracker.semaphore.clone(),
            waiter_thread,
            parameters.max_in_flight_submissions as usize,
        );

        Ok(Self {
            decoder,
            submission_tracker,
            output: Arc::new(Mutex::new(BytesOutput {
                frame_sorter: FrameSorter::default(),
                on_frame_callback,
            })),
        })
    }

    fn download_frame(
        frame: DownloadFrameSubmission<Buffer>,
    ) -> Result<DecodeResult<RawFrameData>, VulkanDecoderError> {
        frame.check_decode_results()?;
        unsafe { frame.output_to_bytes() }
    }
}

impl VideoDecoderBackend for VulkanBytesDecoderH264 {
    fn process_event_bytes(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError> {
        let flush = matches!(event, DecoderEvent::Flush);
        let instructions = self.decoder.process_event(event)?;

        for instruction in instructions {
            self.submission_tracker
                .wait_if_full(timeout)
                .map_err(VulkanDecoderError::from)?;

            let Some(submission) = self.decoder.decode(instruction)? else {
                continue;
            };
            let (frame, semaphore_wait_value) = submission.download_to_buffer()?;

            let command_buffer_pools = self.decoder.decoder.tracker.command_buffer_pools.clone();
            let decode_failed = self.decoder.event_processor.decode_failed_flag();
            let output = self.output.clone();

            self.submission_tracker
                .add_wait_request(semaphore_wait_value, move || {
                    command_buffer_pools.mark_submitted_as_free(semaphore_wait_value);
                    let mut output = output.lock().unwrap();
                    let frame = match VulkanBytesDecoderH264::download_frame(frame) {
                        Ok(frame) => frame,
                        Err(err) => {
                            tracing::debug!("Frame decoding failed: {err}");
                            decode_failed.store(true, Ordering::Relaxed);
                            return;
                        }
                    };

                    output.send_frame(frame);
                })
                .map_err(VulkanDecoderError::from)?;
        }

        if flush {
            self.submission_tracker
                .wait_for_all(timeout)
                .map_err(VulkanDecoderError::from)?;

            let mut output = self.output.lock().unwrap();
            let frames = output.frame_sorter.flush();
            for frame in frames {
                (output.on_frame_callback)(frame);
            }
        }

        Ok(())
    }
}

impl BytesOutput {
    fn send_frame(&mut self, frame: DecodeResult<RawFrameData>) {
        let frames = self.frame_sorter.put(frame);
        for frame in frames {
            (self.on_frame_callback)(frame);
        }
    }
}

#[cfg(feature = "wgpu")]
pub(crate) struct VulkanWgpuTexturesDecoderH264 {
    decoder: VulkanDecoderH264,
    submission_tracker: SubmissionTracker,
    frame_sorter: FrameSorter<VideoTexture>,
    on_frame_callback: Box<dyn FnMut(OutputFrame<VideoTexture>) + Send>,
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,
}

#[cfg(feature = "wgpu")]
impl VulkanWgpuTexturesDecoderH264 {
    pub(crate) fn new(
        decoding_device: Arc<DecodingDevice>,
        parameters: DecoderParameters,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        on_frame_callback: Box<dyn FnMut(OutputFrame<VideoTexture>) + Send>,
        waiter_thread: Arc<WaiterThreadHandle>,
    ) -> Result<Self, VulkanDecoderError> {
        let transfer_queue_idx = decoding_device.queues.transfer.family_index;
        let decoder = VulkanDecoderH264::new(
            decoding_device,
            parameters,
            ImageModifiers {
                additional_queue_index: transfer_queue_idx,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;
        let submission_tracker = SubmissionTracker::new(
            decoder.decoder.tracker.semaphore_tracker.semaphore.clone(),
            waiter_thread,
            parameters.max_in_flight_submissions as usize,
        );

        Ok(Self {
            decoder,
            submission_tracker,
            frame_sorter: FrameSorter::default(),
            on_frame_callback,
            wgpu_device,
            wgpu_queue,
        })
    }
}

#[cfg(feature = "wgpu")]
impl crate::decoders::WgpuVideoDecoderBackend for VulkanWgpuTexturesDecoderH264 {
    fn process_event_textures(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
        timeout: Duration,
    ) -> Result<(), VideoDecoderError> {
        let flush = matches!(event, DecoderEvent::Flush);
        let instructions = self.decoder.process_event(event)?;

        let mut unordered_frames = Vec::new();
        for instruction in instructions {
            self.submission_tracker
                .wait_if_full(timeout)
                .map_err(VulkanDecoderError::from)?;

            let Some(submission) = self.decoder.decode(instruction)? else {
                continue;
            };
            let (frame, semaphore_wait_value) =
                submission.download_to_wgpu_texture(&self.wgpu_device, &self.wgpu_queue)?;

            unordered_frames.push(DecodeResult {
                frame: frame.frame.clone(),
                metadata: frame.decode_metadata,
            });

            let command_buffer_pools = self.decoder.decoder.tracker.command_buffer_pools.clone();
            let decode_failed = self.decoder.event_processor.decode_failed_flag();

            self.submission_tracker
                .add_wait_request(semaphore_wait_value, move || {
                    command_buffer_pools.mark_submitted_as_free(semaphore_wait_value);
                    if let Err(err) = frame.check_decode_results() {
                        tracing::debug!("Frame decoding failed: {err}");
                        decode_failed.store(true, Ordering::Relaxed);
                    }
                })
                .map_err(VulkanDecoderError::from)?;
        }

        let mut ordered_frames = self.frame_sorter.put_frames(unordered_frames);
        if flush {
            ordered_frames.append(&mut self.frame_sorter.flush());
        }

        for frame in ordered_frames {
            (self.on_frame_callback)(frame);
        }

        Ok(())
    }
}
