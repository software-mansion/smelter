use std::sync::Arc;

use ash::vk;

use crate::VulkanCommonError;

use super::Device;

pub(crate) struct TimelineSemaphore {
    pub(crate) semaphore: vk::Semaphore,
    device: Arc<Device>,
}

impl TimelineSemaphore {
    pub(crate) fn new(device: Arc<Device>, initial_value: u64) -> Result<Self, VulkanCommonError> {
        let mut create_type_info = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value);
        let create_info = vk::SemaphoreCreateInfo::default().push_next(&mut create_type_info);

        let semaphore = unsafe { device.create_semaphore(&create_info, None)? };

        Ok(Self { semaphore, device })
    }

    pub(crate) fn wait(&self, timeout: u64, value: u64) -> Result<(), VulkanCommonError> {
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

pub(crate) struct TrackerWait<S> {
    pub(crate) value: u64,
    pub(crate) _state: S,
}

pub(crate) struct Tracker<S> {
    pub(crate) semaphore: TimelineSemaphore,
    next_value: u64,
    pub(crate) wait_for: Option<TrackerWait<S>>,
}

impl<S> Tracker<S> {
    pub(crate) fn new(device: Arc<Device>) -> Result<Self, VulkanCommonError> {
        Ok(Self {
            next_value: 1,
            wait_for: None,
            semaphore: TimelineSemaphore::new(device, 0)?,
        })
    }

    pub(crate) fn next_sem_value(&mut self) -> u64 {
        let val = self.next_value;
        self.next_value += 1;
        val
    }

    /// This is a noop if there's nothing to wait for
    pub(crate) fn wait(&mut self, timeout: u64) -> Result<(), VulkanCommonError> {
        if let Some(wait_for) = self.wait_for.as_ref() {
            self.semaphore.wait(timeout, wait_for.value)?;
            self.wait_for = None;
        }

        Ok(())
    }
}
