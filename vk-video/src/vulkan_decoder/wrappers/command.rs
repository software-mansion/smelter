use std::sync::Arc;

use ash::vk;

use crate::vulkan_decoder::{VulkanCtxError, VulkanDecoderError};

use super::Device;

pub(crate) struct CommandPool {
    pub(crate) command_pool: vk::CommandPool,
    device: Arc<Device>,
}

impl CommandPool {
    pub(crate) fn new(
        device: Arc<Device>,
        queue_family_index: usize,
    ) -> Result<Self, VulkanCtxError> {
        let create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index as u32);

        let command_pool = unsafe { device.create_command_pool(&create_info, None)? };

        Ok(Self {
            device,
            command_pool,
        })
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

impl std::ops::Deref for CommandPool {
    type Target = vk::CommandPool;

    fn deref(&self) -> &Self::Target {
        &self.command_pool
    }
}

pub(crate) struct CommandBuffer {
    pool: Arc<CommandPool>,
    pub(crate) buffer: vk::CommandBuffer,
}

impl CommandBuffer {
    pub(crate) fn new_primary(pool: Arc<CommandPool>) -> Result<Self, VulkanDecoderError> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(**pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let buffer = unsafe { pool.device.allocate_command_buffers(&allocate_info)?[0] };

        Ok(Self { pool, buffer })
    }

    pub(crate) fn begin(&self) -> Result<(), VulkanDecoderError> {
        unsafe {
            self.device().begin_command_buffer(
                self.buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?
        };
        Ok(())
    }

    pub(crate) fn end(&self) -> Result<(), VulkanDecoderError> {
        unsafe { self.device().end_command_buffer(self.buffer)? };

        Ok(())
    }

    fn device(&self) -> &Device {
        &self.pool.device
    }
}

impl std::ops::Deref for CommandBuffer {
    type Target = vk::CommandBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        unsafe {
            self.pool
                .device
                .free_command_buffers(**self.pool, &[self.buffer])
        };
    }
}
