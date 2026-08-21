use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing::error;

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

const SUBMISSION_WAIT_TIMEOUT: Duration = Duration::from_secs(1);

pub(crate) struct VulkanCallbackEncoder<C: EncodeCodec> {
    encoder: VulkanEncoder<'static, C>,
    submission_tracker: SubmissionTracker,
    #[allow(clippy::type_complexity)]
    on_chunk_callback: Arc<Mutex<Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>>>,
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
        })
    }

    fn submit_for_waiting(
        &mut self,
        submission: UnwaitedEncodeSubmission,
    ) -> Result<(), VulkanEncoderError> {
        let on_chunk_callback = self.on_chunk_callback.clone();
        let command_buffer_pools = self.encoder.tracker.command_buffer_pools.clone();
        let wait_value = submission.0.wait_value;
        self.submission_tracker.add_wait_request(
            wait_value,
            SUBMISSION_WAIT_TIMEOUT,
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
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(SUBMISSION_WAIT_TIMEOUT)
            .map_err(VulkanEncoderError::from)?;

        let submission = self.encoder.encode_bytes(frame, force_idr)?;
        self.submit_for_waiting(submission)?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_for_all(SUBMISSION_WAIT_TIMEOUT)
            .map_err(VulkanEncoderError::from)?;

        Ok(())
    }
}

#[cfg(feature = "wgpu")]
impl<C: EncodeCodec + 'static> crate::encoders::WgpuVideoEncoderBackend
    for VulkanCallbackEncoder<C>
{
    fn encode_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<wgpu::Texture>,
        force_idr: bool,
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(SUBMISSION_WAIT_TIMEOUT)
            .map_err(VulkanEncoderError::from)?;

        let submission = self
            .encoder
            .encode_texture(wgpu_device, wgpu_queue, frame, force_idr)?;
        self.submit_for_waiting(submission)?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_for_all(SUBMISSION_WAIT_TIMEOUT)
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
