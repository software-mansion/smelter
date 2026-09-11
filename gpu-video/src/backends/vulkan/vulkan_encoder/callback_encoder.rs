use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ash::vk;
use tracing::error;

use crate::{
    EncodedOutputChunk, InputFrame, RawFrameData,
    backends::vulkan::{
        VulkanEncoder, VulkanEncoderError,
        codec::{EncodeCodec, h264::H264Codec, h265::H265Codec},
        vulkan_device::EncodingDevice,
        vulkan_encoder::{DynVulkanEncoder, EncoderTrackerWaitState, FullEncoderParameters},
        waiter_thread::{SubmissionTracker, WaiterThreadHandle},
        wrappers::{
            Buffer, CommandBufferPoolStorage, EncodeInputImage, EncodeInputImagePool, Image,
        },
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

// TODO: rename
// TODO: Test if transcoding works after changes
pub(crate) struct VulkanCallbackEncoder<'a, C: EncodeCodec> {
    submission_tracker: SubmissionTracker,
    input_image_pool: EncodeInputImagePool<'a>,
    on_chunk_callback: Arc<Mutex<Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>>>,
    // TODO: these should be only needed for wgpu, bytes decoder doesn;t need it
    #[cfg(feature = "wgpu")]
    command_encoder: Option<wgpu::hal::vulkan::CommandEncoder>,
    #[cfg(feature = "wgpu")]
    used_input_images: HashMap<ash::vk::Image, EncodeInputImage>,

    encoder: VulkanEncoder<'a, C>,
    encoding_device: Arc<EncodingDevice>,
}

impl<'a, C: EncodeCodec + 'a> VulkanCallbackEncoder<'a, C> {
    pub(crate) fn new(
        encoding_device: Arc<EncodingDevice>,
        parameters: FullEncoderParameters<C>,
        on_chunk_callback: Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>,
        waiter_thread: Arc<WaiterThreadHandle>,
    ) -> Result<Self, VulkanEncoderError> {
        let max_in_flight_submissions = parameters.max_in_flight_submissions.get() as usize;
        let encoder = VulkanEncoder::new(encoding_device.clone(), parameters)?;
        let submission_tracker = SubmissionTracker::new(
            encoder.tracker.semaphore_tracker.semaphore.clone(),
            waiter_thread,
            max_in_flight_submissions,
        );

        let input_image_queue_families = [
            encoding_device.queues.transfer.family_index as u32,
            encoding_device.queues.wgpu.family_index as u32,
        ];
        let input_image_pool = EncodeInputImagePool::new(
            encoding_device.clone(),
            encoder.profile_info.clone(),
            encoder
                .session_resources
                .video_session
                .max_coded_extent
                .into(),
            input_image_queue_families.into(),
            encoder.tracker.image_layout_tracker.clone(),
        );

        Ok(Self {
            encoder,
            submission_tracker,
            input_image_pool,
            on_chunk_callback: Arc::new(Mutex::new(on_chunk_callback)),
            // TODO: ugh
            #[cfg(feature = "wgpu")]
            command_encoder: None,
            #[cfg(feature = "wgpu")]
            used_input_images: HashMap::new(),
            encoding_device,
        })
    }

    fn transfer_buffer_to_image(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        input_image: &Arc<Image>,
    ) -> Result<Buffer, VulkanEncoderError> {
        let extent = input_image.extent;

        if frame.data.width != extent.width || frame.data.height != extent.height {
            return Err(VulkanEncoderError::WrongFrameDimensions {
                provided: (frame.data.width, frame.data.height),
                expected: (extent.width, extent.height),
            });
        }

        if frame.data.width as usize * frame.data.height as usize * 3 / 2 != frame.data.frame.len()
        {
            return Err(VulkanEncoderError::InconsistentPictureByteSize {
                bytes: frame.data.frame.len(),
                size_from_resolution: frame.data.width as usize * frame.data.height as usize * 3
                    / 2,
            });
        }

        let tracker = &mut self.encoder.tracker;
        let mut cmd_buffer = tracker.command_buffer_pools.transfer.begin_buffer()?;

        input_image.transition_layout_single_layer(
            &mut cmd_buffer,
            vk::PipelineStageFlags2::ALL_COMMANDS..vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::NONE..vk::AccessFlags2::TRANSFER_WRITE,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            0,
        )?;

        let buffer = Buffer::new_transfer_with_data(
            self.encoding_device.allocator.clone(),
            &frame.data.frame,
        )?;

        unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .cmd_copy_buffer_to_image(
                    cmd_buffer.buffer(),
                    *buffer,
                    input_image.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[
                        vk::BufferImageCopy::default()
                            .buffer_offset(0)
                            .buffer_row_length(0)
                            .buffer_image_height(0)
                            .image_subresource(vk::ImageSubresourceLayers {
                                aspect_mask: vk::ImageAspectFlags::PLANE_0,
                                layer_count: 1,
                                base_array_layer: 0,
                                mip_level: 0,
                            })
                            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                            .image_extent(vk::Extent3D {
                                width: extent.width,
                                height: extent.height,
                                depth: 1,
                            }),
                        vk::BufferImageCopy::default()
                            .buffer_offset(extent.width as u64 * extent.height as u64)
                            .buffer_row_length(0)
                            .buffer_image_height(0)
                            .image_subresource(vk::ImageSubresourceLayers {
                                aspect_mask: vk::ImageAspectFlags::PLANE_1,
                                layer_count: 1,
                                base_array_layer: 0,
                                mip_level: 0,
                            })
                            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                            .image_extent(vk::Extent3D {
                                width: extent.width / 2,
                                height: extent.height / 2,
                                depth: 1,
                            }),
                    ],
                );
        }

        self.encoding_device
            .queues
            .transfer
            .submit_chain_semaphore(
                cmd_buffer.end()?,
                tracker,
                vk::PipelineStageFlags2::COPY,
                vk::PipelineStageFlags2::COPY,
                EncoderTrackerWaitState::CopyBufferToImage,
            )?;

        Ok(buffer)
    }

    fn submit_encode(
        &mut self,
        encode_image: EncodeInputImage,
        staging_buffer: Option<Buffer>,
        force_idr: bool,
        pts: Option<u64>,
        timeout: Duration,
    ) -> Result<(), VulkanEncoderError> {
        let submission = self
            .encoder
            .encode(encode_image.image.clone(), force_idr, pts)?;

        let on_chunk_callback = self.on_chunk_callback.clone();
        let command_buffer_pools = self.encoder.tracker.command_buffer_pools.clone();
        let wait_value = submission.0.wait_value;
        self.submission_tracker
            .add_wait_request(wait_value, timeout, move || {
                command_buffer_pools.mark_submitted_as_free(wait_value);
                encode_image.release_to_pool();
                drop(staging_buffer);
                match submission.0.download() {
                    Ok(chunk) => (on_chunk_callback.lock().unwrap())(chunk),
                    Err(err) => error!("Encoding a frame failed: {err}"),
                }
            })
            .map_err(Into::into)
    }

    fn flush(&mut self, timeout: Duration) -> Result<(), VulkanEncoderError> {
        self.submission_tracker
            .wait_for_all(timeout)
            .map_err(Into::into)
    }
}

impl<'a, C: EncodeCodec + 'static> VideoEncoderBackend for VulkanCallbackEncoder<'a, C> {
    // TODO: handle in_flight equal 0
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
        timeout: Duration,
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(timeout)
            .map_err(VulkanEncoderError::from)?;

        let encode_image = self.input_image_pool.vk_image()?;
        let buffer = self.transfer_buffer_to_image(frame, &encode_image.image)?;
        self.submit_encode(encode_image, Some(buffer), force_idr, frame.pts, timeout)?;

        Ok(())
    }

    fn flush(&mut self, timeout: Duration) -> Result<(), VideoEncoderError> {
        Ok(VulkanCallbackEncoder::flush(self, timeout)?)
    }
}

impl VideoEncoderParametersInfoH264 for VulkanCallbackEncoder<'static, H264Codec> {
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.sps()
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.pps()
    }
}

impl VideoEncoderParametersInfoH265 for VulkanCallbackEncoder<'static, H265Codec> {
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
