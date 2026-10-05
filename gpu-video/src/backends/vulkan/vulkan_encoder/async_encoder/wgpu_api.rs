use std::time::Duration;

use ash::vk;
use wgpu::hal::{Queue, vulkan::Api as VkApi};

use crate::{
    InputFrame, VideoTexture,
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
    fn image_from_video_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        video_texture: &VideoTexture,
    ) -> Result<EncodeInputImage, VulkanEncoderError> {
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

        if let Some(wait_for) = self.encoder.tracker.semaphore_tracker.wait_for.as_ref() {
            hal_queue.add_wait_semaphore(
                self.encoder.tracker.raw_semaphore(),
                Some(wait_for.value.0),
                vk::PipelineStageFlags::ALL_COMMANDS,
            );
        }

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
            .encoder
            .tracker
            .semaphore_tracker
            .next_submit_info(EncoderTrackerWaitState::TransitionInputImage);

        unsafe {
            hal_queue
                .submit(&[], &[], semaphore_submit_info.wgpu_signal_info())
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
    ) -> Result<(), VideoEncoderError> {
        self.submission_tracker
            .wait_if_full(Duration::MAX)
            .map_err(VulkanEncoderError::from)?;

        let encode_image =
            self.image_from_video_texture(wgpu_device, wgpu_queue, &frame.data.texture)?;
        self.submit_encode(encode_image, None, force_idr, frame.pts)?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), VideoEncoderError> {
        Ok(AsyncVulkanEncoder::flush(self)?)
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
