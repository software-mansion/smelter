use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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

type OnEncodedChunkCallback = Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>;

pub(crate) struct AsyncVulkanEncoder<'a, C: EncodeCodec> {
    submission_tracker: SubmissionTracker,
    input_image_pool: EncodeInputImagePool<'a>,
    on_chunk_callback: Arc<Mutex<OnEncodedChunkCallback>>,
    encode_failed: Arc<AtomicBool>,

    #[cfg(feature = "wgpu")]
    used_input_images: Arc<Mutex<HashMap<wgpu::Texture, EncodeInputImage>>>,

    encoder: VulkanEncoder<'a, C>,
    encoding_device: Arc<EncodingDevice>,
}

impl<'a, C: EncodeCodec + 'a> AsyncVulkanEncoder<'a, C> {
    pub(crate) fn new(
        encoding_device: Arc<EncodingDevice>,
        parameters: FullEncoderParameters<C>,
        on_chunk_callback: OnEncodedChunkCallback,
        waiter_thread: Arc<WaiterThreadHandle>,
    ) -> Result<Self, VulkanEncoderError> {
        let max_in_flight = parameters.max_in_flight_submissions as usize;
        let encoder = VulkanEncoder::new(encoding_device.clone(), parameters)?;
        let submission_tracker = SubmissionTracker::new(
            encoder.tracker.semaphore_tracker.semaphore.clone(),
            waiter_thread,
            max_in_flight,
        );

        let input_image_queue_families = vec![
            encoding_device.queues.transfer.family_index as u32,
            encoding_device.queues.wgpu.family_index as u32,
        ];

        let encode_image_usages = match cfg!(feature = "wgpu") {
            true => {
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_DST
            }
            false => vk::ImageUsageFlags::TRANSFER_DST,
        };
        let input_image_pool = EncodeInputImagePool::new(
            encoding_device.clone(),
            encoder.profile_info.clone(),
            encoder
                .session_resources
                .video_session
                .max_coded_extent
                .into(),
            encode_image_usages,
            input_image_queue_families,
            encoder.tracker.image_layout_tracker.clone(),
        );

        Ok(Self {
            encoder,
            submission_tracker,
            input_image_pool,
            on_chunk_callback: Arc::new(Mutex::new(on_chunk_callback)),
            #[cfg(feature = "wgpu")]
            used_input_images: Arc::new(Mutex::new(HashMap::new())),
            encoding_device,
            encode_failed: Arc::new(AtomicBool::new(false)),
        })
    }

    fn transfer_buffer_to_image(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        input_image: &Arc<Image>,
    ) -> Result<Buffer, VulkanEncoderError> {
        let extent = input_image.extent;

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
    ) -> Result<(), VulkanEncoderError> {
        let force_idr = force_idr || self.encode_failed.swap(false, Ordering::Relaxed);
        let submission = self
            .encoder
            .encode(encode_image.image.clone(), force_idr, pts)?;

        let on_chunk_callback = self.on_chunk_callback.clone();
        let command_buffer_pools = self.encoder.tracker.command_buffer_pools.clone();
        let wait_value = submission.0.wait_value;
        let encode_failed = self.encode_failed.clone();

        self.submission_tracker
            .add_wait_request(wait_value, move || {
                command_buffer_pools.mark_submitted_as_free(wait_value);
                encode_image.release_to_pool();
                drop(staging_buffer);
                match submission.0.download() {
                    Ok(chunk) => (on_chunk_callback.lock().unwrap())(chunk),
                    Err(err) => {
                        error!("Encoding a frame failed: {err}");
                        encode_failed.store(true, Ordering::Relaxed);
                    }
                }
            })
            .map_err(VulkanEncoderError::from)
    }

    fn flush(&mut self, timeout: Duration) -> Result<(), VulkanEncoderError> {
        Ok(self.submission_tracker.wait_for_all(timeout)?)
    }
}

impl<'a, C: EncodeCodec + 'static> VideoEncoderBackend for AsyncVulkanEncoder<'a, C> {
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
        timeout: Duration,
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(timeout)
            .map_err(VulkanEncoderError::from)?;

        let encode_image = self.input_image_pool.image()?;
        let buffer = self.transfer_buffer_to_image(frame, &encode_image.image)?;
        self.submit_encode(encode_image, Some(buffer), force_idr, frame.pts)?;

        Ok(())
    }

    fn flush(&mut self, timeout: Duration) -> Result<(), VideoEncoderError> {
        Ok(AsyncVulkanEncoder::flush(self, timeout)?)
    }
}

impl VideoEncoderParametersInfoH264 for AsyncVulkanEncoder<'static, H264Codec> {
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.sps()
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.pps()
    }
}

impl VideoEncoderParametersInfoH265 for AsyncVulkanEncoder<'static, H265Codec> {
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
