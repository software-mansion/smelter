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
#[cfg(feature = "wgpu")]
use crate::{backends::vulkan::{vulkan_encoder::DynVulkanEncoder, wrappers::EncodeInputImage}, encoders::EncodeTexture};

// TODO: Test if transcoding works after changes
pub(crate) struct VulkanCallbackEncoder<C: EncodeCodec> {
    encoder: VulkanEncoder<'static, C>,
    submission_tracker: SubmissionTracker,
    on_chunk_callback: Arc<Mutex<Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>>>,
    #[cfg(feature = "wgpu")]
    command_encoder: Option<wgpu::hal::vulkan::CommandEncoder>,
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

    // TODO: rename
    #[cfg(feature = "wgpu")]
    fn encode_image_from_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: EncodeTexture,
    ) -> Result<Box<EncodeInputImage>, VulkanEncoderError> {
        use std::any::Any;

        use crate::{
            backends::vulkan::vulkan_encoder::EncoderTrackerWaitState,
            encoders::WgpuTextureEncoderError,
        };
        use ash::vk;
        use wgpu::hal::{CommandEncoder, Device, Queue, vulkan::Api as VkApi};

        let hal_device = unsafe { wgpu_device.as_hal::<VkApi>().unwrap() };
        let hal_queue = unsafe { wgpu_queue.as_hal::<VkApi>().unwrap() };

        // TODO: eeeeeeh
        let wgpu_texture = frame.wgpu_texture;
        let input_image = (frame.backend_texture as Box<dyn Any>)
            .downcast::<EncodeInputImage>()
            .unwrap();

        // TODO: any way to skip this?
        // TODO: what if the texture was transitioned in a different thread?
        let mut encoder = wgpu_device.create_command_encoder(&Default::default());
        encoder.transition_resources(
            [].into_iter(),
            [wgpu::TextureTransition {
                texture: &wgpu_texture,
                selector: None,
                state: wgpu::TextureUses::RESOURCE,
            }]
            .into_iter(),
        );
        wgpu_queue.submit([encoder.finish()]);

        // :(
        self.encoder
            .tracker
            .image_layout_tracker
            .lock()
            .unwrap()
            .map
            .insert(
                input_image.image.key(),
                vec![vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL].into_boxed_slice(),
            );

        // wgpu core queue makes it impossible to specify signal semaphores
        // so we have to make an empty submit on the wgpu hal queue just for the synchronization
        let hal_encoder = self.command_encoder.get_or_insert_with(|| unsafe {
            hal_device
                .create_command_encoder(&wgpu::hal::CommandEncoderDescriptor {
                    label: Some("gpu-video: transition encoder image layout"),
                    queue: &hal_queue,
                })
                .unwrap()
        });

        let command_buffer = unsafe {
            hal_encoder
                .begin_encoding(None)
                .map_err(WgpuTextureEncoderError::from)?;
            hal_encoder
                .end_encoding()
                .map_err(WgpuTextureEncoderError::from)?
        };

        let mut semaphore_submit_info = self
            .encoder
            .tracker
            .semaphore_tracker
            .next_submit_info(EncoderTrackerWaitState::CopyImageToImage);
        unsafe {
            hal_queue
                .submit(
                    &[&command_buffer],
                    &[],
                    semaphore_submit_info.wgpu_wait_info(),
                )
                .map_err(WgpuTextureEncoderError::from)?;
        }

        semaphore_submit_info.mark_submitted();
        Ok(input_image)
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

#[cfg(feature = "wgpu")]
impl<C: EncodeCodec + 'static> crate::encoders::WgpuVideoEncoderBackend
    for VulkanCallbackEncoder<C>
{
    // TODO: timeout
    // TODO: handle in_flight equal 0
    fn encode_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<EncodeTexture>,
        force_idr: bool,
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(Duration::from_secs(1))
            .map_err(VulkanEncoderError::from)?;

        let encode_image = self.encode_image_from_texture(wgpu_device, wgpu_queue, frame.data)?;
        let mut submission = self.encoder.encode(encode_image.image.clone(), force_idr, frame.pts)?;

        // TODO: i don't like it too
        submission.0.in_flight_resources.input_image = Some(encode_image);
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

    fn next_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VideoEncoderError> {
        let texture = self.encoder.input_image_pool.wgpu_texture(wgpu_device)?;
        Ok(EncodeTexture {
            wgpu_texture: texture.wgpu_texture.clone().unwrap(), // TODO: unwrap :(
            backend_texture: Box::new(texture),
        })
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
