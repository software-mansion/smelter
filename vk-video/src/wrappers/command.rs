use std::sync::{Arc, Mutex};

use ash::vk::{self, Handle};

use crate::{VulkanCommonError, VulkanDevice};

struct CommandPool {
    command_pool: vk::CommandPool,
    device: Arc<VulkanDevice>,
}

impl CommandPool {
    fn new(
        device: Arc<VulkanDevice>,
        queue_family_index: usize,
    ) -> Result<Self, VulkanCommonError> {
        let create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index as u32);

        let command_pool = unsafe { device.device.create_command_pool(&create_info, None)? };

        Ok(Self {
            device,
            command_pool,
        })
    }

    fn new_primary(&self) -> Result<vk::CommandBuffer, VulkanCommonError> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(**self)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let buffer = unsafe {
            self.device
                .device
                .allocate_command_buffers(&allocate_info)?[0]
        };

        Ok(buffer)
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device
                .device
                .destroy_command_pool(self.command_pool, None);
        }
    }
}

impl std::ops::Deref for CommandPool {
    type Target = vk::CommandPool;

    fn deref(&self) -> &Self::Target {
        &self.command_pool
    }
}

pub(crate) struct CommandBufferPool(Arc<Mutex<CommandBufferPoolInner>>);

pub(crate) struct CommandBufferPoolInner {
    command_pool: CommandPool,
    free: Vec<vk::CommandBuffer>,
    submitted: Vec<vk::CommandBuffer>,
}

impl CommandBufferPool {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        queue_family_index: usize,
    ) -> Result<Self, VulkanCommonError> {
        let command_pool = CommandPool::new(device, queue_family_index)?;

        Ok(Self(Arc::new(Mutex::new(CommandBufferPoolInner {
            command_pool,
            free: Vec::new(),
            submitted: Vec::new(),
        }))))
    }

    pub(crate) fn begin_buffer(&self) -> Result<OpenCommandBuffer, VulkanCommonError> {
        let mut inner = self.0.lock().unwrap();
        let buffer = match inner.free.pop() {
            Some(buffer) => buffer,
            None => inner.command_pool.new_primary()?,
        };

        unsafe {
            inner.command_pool.device.device.begin_command_buffer(
                buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?
        };

        Ok(OpenCommandBuffer(UnfinishedCommandBuffer {
            buffer,
            pool: self.0.clone(),
        }))
    }

    pub(crate) fn mark_submitted_as_free(&self) {
        let mut guard = self.0.lock().unwrap();
        let inner = &mut *guard;
        inner.free.append(&mut inner.submitted);
    }
}

struct UnfinishedCommandBuffer {
    buffer: vk::CommandBuffer,
    pool: Arc<Mutex<CommandBufferPoolInner>>,
}

impl UnfinishedCommandBuffer {
    fn destroy_without_reset(self) {
        std::mem::forget(self);
    }
}

impl Drop for UnfinishedCommandBuffer {
    fn drop(&mut self) {
        let mut locked = self.pool.lock().unwrap();

        unsafe {
            if let Err(e) = locked
                .command_pool
                .device
                .device
                .reset_command_buffer(self.buffer, vk::CommandBufferResetFlags::empty())
            {
                tracing::error!(
                    "Open command buffer {:x} failed when resetting: {e}. Something is very wrong",
                    self.buffer.as_raw()
                );
            }
        }

        locked.free.push(self.buffer);
    }
}

pub(crate) struct OpenCommandBuffer(UnfinishedCommandBuffer);

impl OpenCommandBuffer {
    pub(crate) fn end(self) -> Result<RecordedCommandBuffer, VulkanCommonError> {
        let buffer = self.0.buffer;
        unsafe {
            self.0
                .pool
                .lock()
                .unwrap()
                .command_pool
                .device
                .device
                .end_command_buffer(buffer)?
        }

        Ok(RecordedCommandBuffer(self.0))
    }

    pub(crate) fn buffer(&self) -> vk::CommandBuffer {
        self.0.buffer
    }
}

pub(crate) struct RecordedCommandBuffer(UnfinishedCommandBuffer);

impl RecordedCommandBuffer {
    pub(crate) fn mark_submitted(self) {
        self.0.pool.lock().unwrap().submitted.push(self.0.buffer);
        self.0.destroy_without_reset();
    }

    pub(crate) fn buffer(&self) -> vk::CommandBuffer {
        self.0.buffer
    }
}
