use std::time::Duration;

use ash::vk;
use wgpu::hal::{Queue, vulkan::Api as VkApi};

use crate::{
    InputFrame,
    backends::vulkan::{
        VulkanEncoderError,
        codec::EncodeCodec,
        vulkan_encoder::{EncoderTrackerWaitState, async_encoder::AsyncVulkanEncoder},
        wrappers::EncodeInputImage,
    },
    encoders::{
        EncodeTexture, VideoEncoderError, WgpuTextureEncoderError, WgpuVideoEncoderBackend,
    },
};

impl<'a, C: EncodeCodec> AsyncVulkanEncoder<'a, C> {
    fn encode_image_from_wgpu_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        wgpu_texture: &wgpu::Texture,
    ) -> Result<EncodeInputImage, VulkanEncoderError> {
        let hal_queue = unsafe { wgpu_queue.as_hal::<VkApi>().unwrap() };

        let input_image = self
            .used_input_images
            .lock()
            .unwrap()
            .remove(wgpu_texture)
            .ok_or(WgpuTextureEncoderError::TextureNotFromEncoder)?;

        let mut encoder = wgpu_device.create_command_encoder(&Default::default());
        encoder.transition_resources(
            [].into_iter(),
            [wgpu::TextureTransition {
                texture: wgpu_texture,
                selector: None,
                state: wgpu::TextureUses::RESOURCE,
            }]
            .into_iter(),
        );
        wgpu_queue.submit([encoder.finish()]);

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

        let mut semaphore_submit_info = self
            .encoder
            .tracker
            .semaphore_tracker
            .next_submit_info(EncoderTrackerWaitState::CopyImageToImage);
        unsafe {
            hal_queue
                .submit(&[], &[], semaphore_submit_info.wgpu_wait_info())
                .map_err(WgpuTextureEncoderError::from)?;
        }

        semaphore_submit_info.mark_submitted();
        Ok(input_image)
    }
}

impl<'a, C: EncodeCodec + 'a> WgpuVideoEncoderBackend for AsyncVulkanEncoder<'a, C> {
    fn encode_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<EncodeTexture>,
        force_idr: bool,
        timeout: Duration,
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(timeout)
            .map_err(VulkanEncoderError::from)?;

        let encode_image =
            self.encode_image_from_wgpu_texture(wgpu_device, wgpu_queue, &frame.data.wgpu_texture)?;
        self.submit_encode(encode_image, None, force_idr, frame.pts, timeout)?;

        Ok(())
    }

    fn flush(&mut self, timeout: Duration) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_for_all(timeout)
            .map_err(VulkanEncoderError::from)?;

        Ok(())
    }

    fn next_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VideoEncoderError> {
        let (image, wgpu_texture) = self.input_image_pool.image_with_wgpu_texture(wgpu_device)?;
        self.used_input_images
            .lock()
            .unwrap()
            .insert(wgpu_texture.clone(), image);

        let used_input_images = self.used_input_images.clone();
        Ok(EncodeTexture {
            wgpu_texture: wgpu_texture.clone(),
            on_drop: Some(Box::new(move || {
                used_input_images.lock().unwrap().remove(&wgpu_texture);
            })),
        })
    }
}
