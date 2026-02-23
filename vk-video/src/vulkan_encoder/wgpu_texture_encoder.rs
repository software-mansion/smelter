use ash::vk;
use wgpu::hal::{CommandEncoder, Device, Queue, vulkan::Api as VkApi};

use crate::{EncodedOutputChunk, Frame, VulkanCommonError, wrappers::TrackerWait};

use super::{EncoderTrackerWaitState, VulkanEncoder, VulkanEncoderError};

#[derive(Debug, thiserror::Error)]
pub enum WgpuEncoderError {
    #[error("The supplied textures format is {0:?}, when it should be NV12")]
    NotNV12Texture(wgpu::TextureFormat),

    #[error(
        "The dimensions of the provided frame ({provided_dimensions:?}) are not the same as the expected dimensions ({expected_dimensions:?})"
    )]
    InconsistentPictureDimensions {
        provided_dimensions: wgpu::Extent3d,
        expected_dimensions: wgpu::Extent3d,
    },

    #[error("Wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error(transparent)]
    VulkanCommonError(#[from] VulkanCommonError),
}

impl VulkanEncoder<'_> {
    fn copy_wgpu_texture_to_image(
        &mut self,
        frame: &Frame<wgpu::Texture>,
    ) -> Result<wgpu::hal::vulkan::CommandEncoder, WgpuEncoderError> {
        if frame.data.format() != wgpu::TextureFormat::NV12 {
            return Err(WgpuEncoderError::NotNV12Texture(frame.data.format()));
        }

        let input_image_size = wgpu::Extent3d {
            width: self.input_image.extent.width,
            height: self.input_image.extent.height,
            depth_or_array_layers: self.input_image.extent.depth,
        };
        if frame.data.size() != input_image_size {
            return Err(WgpuEncoderError::InconsistentPictureDimensions {
                provided_dimensions: frame.data.size(),
                expected_dimensions: input_image_size,
            });
        }

        let wgpu_device = unsafe {
            self.encoding_device
                .wgpu_device()
                .as_hal::<VkApi>()
                .unwrap()
        };
        let wgpu_queue = unsafe { self.encoding_device.wgpu_queue().as_hal::<VkApi>().unwrap() };
        let frame_image = unsafe { frame.data.as_hal::<VkApi>().unwrap().raw_handle() };

        let mut encoder = unsafe {
            wgpu_device.create_command_encoder(&wgpu::hal::CommandEncoderDescriptor {
                label: None,
                queue: &wgpu_queue,
            })
        }?;

        unsafe { encoder.begin_encoding(None)? }
        let buffer = unsafe { encoder.raw_handle() };

        // TODO: This should be abstracted away to some helper function
        let mut layout = self
            .tracker
            .image_layout_tracker
            .lock()
            .unwrap()
            .map
            .get(&self.input_image.key())
            .unwrap()
            .clone();

        self.input_image.transition_layout_raw(
            buffer,
            &mut layout,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::NONE..vk::AccessFlags2::TRANSFER_WRITE,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
        )?;

        unsafe {
            self.encoding_device.device.cmd_copy_image2(
                buffer,
                &vk::CopyImageInfo2::default()
                    .src_image(frame_image)
                    .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .dst_image(self.input_image.image)
                    .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .regions(&[
                        vk::ImageCopy2::default()
                            .src_subresource(
                                vk::ImageSubresourceLayers::default()
                                    .aspect_mask(vk::ImageAspectFlags::PLANE_0)
                                    .mip_level(0)
                                    .base_array_layer(0)
                                    .layer_count(1),
                            )
                            .dst_subresource(
                                vk::ImageSubresourceLayers::default()
                                    .aspect_mask(vk::ImageAspectFlags::PLANE_0)
                                    .mip_level(0)
                                    .base_array_layer(0)
                                    .layer_count(1),
                            )
                            .extent(self.input_image.extent),
                        vk::ImageCopy2::default()
                            .src_subresource(
                                vk::ImageSubresourceLayers::default()
                                    .aspect_mask(vk::ImageAspectFlags::PLANE_1)
                                    .mip_level(0)
                                    .base_array_layer(0)
                                    .layer_count(1),
                            )
                            .dst_subresource(
                                vk::ImageSubresourceLayers::default()
                                    .aspect_mask(vk::ImageAspectFlags::PLANE_1)
                                    .mip_level(0)
                                    .base_array_layer(0)
                                    .layer_count(1),
                            )
                            .extent(vk::Extent3D {
                                width: self.input_image.extent.width / 2,
                                height: self.input_image.extent.height / 2,
                                depth: 1,
                            }),
                    ]),
            )
        };

        self.tracker.wait(u64::MAX)?;

        let mut wgpu_fence = wgpu::hal::vulkan::Fence::TimelineSemaphore(
            self.tracker.semaphore_tracker.semaphore.semaphore,
        );
        let signal_value = self.tracker.semaphore_tracker.next_sem_value();

        unsafe {
            wgpu_queue.submit(
                &[&encoder.end_encoding()?],
                &[],
                (&mut wgpu_fence, signal_value),
            )?;
        }

        self.tracker.semaphore_tracker.wait_for = Some(TrackerWait {
            value: signal_value,
            _state: EncoderTrackerWaitState::CopyImageToImage,
        });

        self.tracker
            .image_layout_tracker
            .lock()
            .unwrap()
            .map
            .insert(self.input_image.key(), layout);

        Ok(encoder)
    }

    /// # Safety
    /// - The texture cannot be a surface texture
    /// - The texture has to be transitioned to [`wgpu::TextureUses::COPY_SRC`] usage
    pub unsafe fn encode_texture(
        &mut self,
        frame: Frame<wgpu::Texture>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        let _cmd_encoder = self.copy_wgpu_texture_to_image(&frame)?;

        let is_keyframe = force_idr || self.idr_period_counter == 0;
        let result = self.encode(self.input_image.clone(), force_idr)?;

        Ok(EncodedOutputChunk {
            data: result,
            pts: frame.pts,
            is_keyframe,
        })
    }
}
