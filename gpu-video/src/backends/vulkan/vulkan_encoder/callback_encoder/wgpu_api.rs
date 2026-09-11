use std::time::Duration;

use ash::vk;
use wgpu::hal::{CommandEncoder, Device, Queue, vulkan::Api as VkApi};

use crate::{
    InputFrame,
    backends::vulkan::{
        VulkanEncoderError,
        codec::EncodeCodec,
        vulkan_encoder::{
            DynVulkanEncoder, EncoderTrackerWaitState, callback_encoder::VulkanCallbackEncoder,
        },
        wrappers::EncodeInputImage,
    },
    encoders::{
        EncodeTexture, VideoEncoderError, WgpuTextureEncoderError, WgpuVideoEncoderBackend,
    },
};

impl<C: EncodeCodec + 'static> VulkanCallbackEncoder<C> {
    // TODO: rename
    fn encode_image_from_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        texture: EncodeTexture,
    ) -> Result<Box<EncodeInputImage>, VulkanEncoderError> {
        let hal_device = unsafe { wgpu_device.as_hal::<VkApi>().unwrap() };
        let hal_queue = unsafe { wgpu_queue.as_hal::<VkApi>().unwrap() };

        let key = unsafe { texture.as_hal::<VkApi>() }
            .map(|hal_texture| unsafe { hal_texture.raw_handle() })
            .ok_or(WgpuTextureEncoderError::TextureNotFromEncoder)?;
        let input_image = Box::new(
            self.used_input_images
                .remove(&key)
                .ok_or(WgpuTextureEncoderError::TextureNotFromEncoder)?,
        );
        let wgpu_texture = texture.0;

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

impl<C: EncodeCodec + 'static> WgpuVideoEncoderBackend for VulkanCallbackEncoder<C> {
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
        let mut submission =
            self.encoder
                .encode(encode_image.image.clone(), force_idr, frame.pts)?;

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
        let image = self.encoder.input_image_pool.wgpu_texture(wgpu_device)?;
        let wgpu_texture = image.wgpu_texture.clone().unwrap();
        self.used_input_images.insert(image.image.image, image);
        Ok(EncodeTexture(wgpu_texture))
    }
}
