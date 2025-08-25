use std::sync::Arc;

use ash::vk;

use crate::VulkanCommonError;

use super::Device;

pub(crate) struct Fence {
    pub(crate) fence: vk::Fence,
    device: Arc<Device>,
}

impl Fence {
    pub(crate) fn new(device: Arc<Device>, signaled: bool) -> Result<Self, VulkanCommonError> {
        let flags = if signaled {
            vk::FenceCreateFlags::SIGNALED
        } else {
            vk::FenceCreateFlags::empty()
        };
        let create_info = vk::FenceCreateInfo::default().flags(flags);
        let fence = unsafe { device.create_fence(&create_info, None)? };

        Ok(Self { device, fence })
    }

    pub(crate) fn wait(&self, timeout: u64) -> Result<(), VulkanCommonError> {
        unsafe { self.device.wait_for_fences(&[self.fence], true, timeout)? };
        Ok(())
    }

    pub(crate) fn reset(&self) -> Result<(), VulkanCommonError> {
        unsafe { self.device.reset_fences(&[self.fence])? };
        Ok(())
    }

    pub(crate) fn wait_and_reset(&self, timeout: u64) -> Result<(), VulkanCommonError> {
        self.wait(timeout)?;
        self.reset()?;

        Ok(())
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        unsafe { self.device.destroy_fence(self.fence, None) };
    }
}

impl std::ops::Deref for Fence {
    type Target = vk::Fence;

    fn deref(&self) -> &Self::Target {
        &self.fence
    }
}

pub(crate) struct Semaphore {
    pub(crate) semaphore: vk::Semaphore,
    device: Arc<Device>,
}

impl Semaphore {
    pub(crate) fn new(device: Arc<Device>) -> Result<Self, VulkanCommonError> {
        let create_info = vk::SemaphoreCreateInfo::default();
        let semaphore = unsafe { device.create_semaphore(&create_info, None)? };

        Ok(Self { device, semaphore })
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe { self.device.destroy_semaphore(self.semaphore, None) };
    }
}

impl std::ops::Deref for Semaphore {
    type Target = vk::Semaphore;

    fn deref(&self) -> &Self::Target {
        &self.semaphore
    }
}
