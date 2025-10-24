use std::sync::Arc;

use ash::vk;

use crate::VulkanCommonError;

use super::Device;

pub struct TimelineSemaphore {
    pub semaphore: vk::Semaphore,
    device: Arc<Device>,
}

impl TimelineSemaphore {
    pub fn new(device: Arc<Device>, initial_value: u64) -> Result<Self, VulkanCommonError> {
        let mut create_type_info = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value);
        let create_info = vk::SemaphoreCreateInfo::default().push_next(&mut create_type_info);

        let semaphore = unsafe { device.create_semaphore(&create_info, None)? };

        Ok(Self { semaphore, device })
    }

    pub fn wait(&self, timeout: u64, value: u64) -> Result<(), VulkanCommonError> {
        let wait_info = vk::SemaphoreWaitInfo::default()
            .semaphores(std::slice::from_ref(&self.semaphore))
            .values(std::slice::from_ref(&value));

        unsafe { self.device.wait_semaphores(&wait_info, timeout)? };

        Ok(())
    }
}

impl Drop for TimelineSemaphore {
    fn drop(&mut self) {
        unsafe { self.device.destroy_semaphore(self.semaphore, None) };
    }
}

pub struct TrackerWait<S> {
    pub value: u64,
    pub _state: S,
}

pub struct Tracker<S> {
    pub semaphore: TimelineSemaphore,
    next_value: u64,
    pub wait_for: Option<TrackerWait<S>>,
}

impl<S> Tracker<S> {
    pub fn new(device: Arc<Device>) -> Result<Self, VulkanCommonError> {
        Ok(Self {
            next_value: 1,
            wait_for: None,
            semaphore: TimelineSemaphore::new(device, 0)?,
        })
    }

    pub fn next_sem_value(&mut self) -> u64 {
        let val = self.next_value;
        self.next_value += 1;
        val
    }

    /// This is a noop if there's nothing to wait for
    pub fn wait(&mut self, timeout: u64) -> Result<(), VulkanCommonError> {
        if let Some(wait_for) = self.wait_for.as_ref() {
            self.semaphore.wait(timeout, wait_for.value)?;
            self.wait_for = None;
        }

        Ok(())
    }
}
