use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing::error;

#[cfg(feature = "wgpu")]
use crate::backends::vulkan::wrappers::EncodeInputImage;
use crate::{
    EncodedOutputChunk, InputFrame, RawFrameData,
    backends::vulkan::{
        VulkanEncoder, VulkanEncoderError,
        codec::{EncodeCodec, h264::H264Codec, h265::H265Codec},
        vulkan_device::EncodingDevice,
        vulkan_encoder::{FullEncoderParameters, UnwaitedEncodeSubmission},
        waiter_thread::{SubmissionTracker, WaiterThreadHandle},
        wrappers::CommandBufferPoolStorage,
    },
    encoders::{
        VideoEncoderBackend, VideoEncoderError, VideoEncoderParametersInfoH264,
        VideoEncoderParametersInfoH265,
    },
};
#[cfg(feature = "wgpu")]
use std::collections::HashMap;

#[cfg(feature = "wgpu")]
mod wgpu_api;

// TODO: Test if transcoding works after changes
pub(crate) struct VulkanCallbackEncoder<C: EncodeCodec> {
    encoder: VulkanEncoder<'static, C>,
    submission_tracker: SubmissionTracker,
    on_chunk_callback: Arc<Mutex<Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>>>,
    #[cfg(feature = "wgpu")]
    command_encoder: Option<wgpu::hal::vulkan::CommandEncoder>,
    #[cfg(feature = "wgpu")]
    used_input_images: HashMap<ash::vk::Image, EncodeInputImage>,
}

impl<C: EncodeCodec + 'static> VulkanCallbackEncoder<C> {
    pub(crate) fn new(
        encoding_device: Arc<EncodingDevice>,
        parameters: FullEncoderParameters<C>,
        on_chunk_callback: Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>,
        waiter_thread: Arc<WaiterThreadHandle>,
    ) -> Result<Self, VulkanEncoderError> {
        let max_in_flight_submissions = parameters.max_in_flight_submissions.get() as usize;
        let encoder = VulkanEncoder::new(encoding_device, parameters)?;
        let submission_tracker = SubmissionTracker::new(
            encoder.tracker.semaphore_tracker.semaphore.clone(),
            waiter_thread,
            max_in_flight_submissions,
        );

        Ok(Self {
            encoder,
            submission_tracker,
            on_chunk_callback: Arc::new(Mutex::new(on_chunk_callback)),
            // TODO: ugh
            #[cfg(feature = "wgpu")]
            command_encoder: None,
            #[cfg(feature = "wgpu")]
            used_input_images: HashMap::new(),
        })
    }

    // TODO: add timeout
    fn submit_for_waiting(
        &mut self,
        submission: UnwaitedEncodeSubmission,
    ) -> Result<(), VulkanEncoderError> {
        let on_chunk_callback = self.on_chunk_callback.clone();
        let command_buffer_pools = self.encoder.tracker.command_buffer_pools.clone();
        let wait_value = submission.0.wait_value;
        self.submission_tracker.add_wait_request(
            wait_value,
            Duration::from_secs(1),
            move || {
                command_buffer_pools.mark_submitted_as_free(wait_value);
                match submission.0.download() {
                    Ok(chunk) => (on_chunk_callback.lock().unwrap())(chunk),
                    Err(err) => error!("Encoding a frame failed: {err}"),
                }
            },
        )?;

        Ok(())
    }
}

impl<C: EncodeCodec + 'static> VideoEncoderBackend for VulkanCallbackEncoder<C> {
    // TODO: timeout
    // TODO: handle in_flight equal 0
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(Duration::from_secs(1))
            .map_err(VulkanEncoderError::from)?;

        let submission = self.encoder.encode_bytes(frame, force_idr)?;
        self.submit_for_waiting(submission)?;

        Ok(())
    }

    // TODO: timeout
    fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_for_all(Duration::from_secs(1))
            .map_err(VulkanEncoderError::from)?;

        Ok(())
    }
}

impl VideoEncoderParametersInfoH264 for VulkanCallbackEncoder<H264Codec> {
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.sps()
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.pps()
    }
}

impl VideoEncoderParametersInfoH265 for VulkanCallbackEncoder<H265Codec> {
    fn vps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.vps()
    }

    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.sps()
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.pps()
    }
}
