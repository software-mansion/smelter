use std::time::Duration;

use ash::vk;
use wgpu::hal::{Queue, vulkan::Api as VkApi};

use crate::{
    InputFrame, VideoTexture,
    backends::vulkan::{
        VulkanEncoderError,
        codec::EncodeCodec,
        vulkan_encoder::{EncoderTrackerWaitState, async_encoder::AsyncVulkanEncoder},
        wrappers::{EncodeInputImage, SemaphoreWaitValue},
    },
    encoders::{
        EncodeTexture, VideoEncoderError, WgpuTextureEncoderError, WgpuVideoEncoderBackend,
    },
};

impl<'a, C: EncodeCodec> AsyncVulkanEncoder<'a, C> {
    fn image_from_video_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        video_texture: &VideoTexture,
    ) -> Result<(EncodeInputImage, SemaphoreWaitValue), VulkanEncoderError> {
        let hal_queue = unsafe { wgpu_queue.as_hal::<VkApi>().unwrap() };

        let input_image = self
            .used_input_images
            .lock()
            .unwrap()
            .remove(video_texture)
            .ok_or(WgpuTextureEncoderError::TextureNotFromEncoder)?;

        let Some(wgpu_texture) = video_texture.nv12_texture() else {
            return Err(WgpuTextureEncoderError::TextureNotFromEncoder.into());
        };

        // Transitioning to a known layout
        let mut encoder = wgpu_device.create_command_encoder(&Default::default());
        encoder.transition_resources(
            [].into_iter(),
            [wgpu::TextureTransition {
                texture: wgpu_texture,
                selector: None,
                state: wgpu::TextureUses::COPY_DST,
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
                vec![vk::ImageLayout::TRANSFER_DST_OPTIMAL].into_boxed_slice(),
            );

        let semaphore_submit_info = self
            .input_semaphore_tracker
            .next_submit_info(EncoderTrackerWaitState::TransitionInputImage);

        unsafe {
            hal_queue
                .submit(&[], &[], semaphore_submit_info.wgpu_signal_info())
                .map_err(WgpuTextureEncoderError::from)?;
        }

        let input_wait_value = semaphore_submit_info.signal_value();
        semaphore_submit_info.mark_submitted();
        Ok((input_image, input_wait_value))
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

        let (encode_image, input_wait_value) =
            self.image_from_video_texture(wgpu_device, wgpu_queue, &frame.data.texture)?;
        self.submit_encode(
            encode_image,
            Some(input_wait_value),
            None,
            force_idr,
            frame.pts,
        )?;

        Ok(())
    }

    fn flush(&mut self, timeout: Duration) -> Result<(), VideoEncoderError> {
        Ok(AsyncVulkanEncoder::flush(self, timeout)?)
    }

    fn next_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VideoEncoderError> {
        let (image, texture) = self.input_image_pool.image_with_wgpu_texture(wgpu_device)?;
        let texture = VideoTexture::from_nv12_texture(texture);
        self.used_input_images
            .lock()
            .unwrap()
            .insert(texture.clone(), image);

        let used_input_images = self.used_input_images.clone();
        Ok(EncodeTexture {
            texture: texture.clone(),
            on_drop: Some(Box::new(move || {
                used_input_images.lock().unwrap().remove(&texture);
            })),
        })
    }
}
