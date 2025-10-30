use std::{
    collections::hash_map::Entry,
    sync::{Arc, Mutex},
};

use ash::vk;
use rustc_hash::FxHashMap;

use crate::{VulkanCommonError, wrappers::ImageKey};

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

pub(crate) trait TrackerKind {
    type WaitState;
    type CommandBufferPools: CommandBufferPoolStorage;
}

pub(crate) trait CommandBufferPoolStorage: Sized {
    fn mark_submitted_as_free(&mut self);
}

pub(crate) struct TrackerWait<S> {
    pub(crate) value: u64,
    pub(crate) _state: S,
}

pub(crate) struct Tracker<K: TrackerKind> {
    pub(crate) semaphore_tracker: SemaphoreTracker<K::WaitState>,
    pub(crate) command_buffer_pools: K::CommandBufferPools,
    pub(crate) image_layout_tracker: Arc<Mutex<ImageLayoutTracker>>,
}

impl<K: TrackerKind> Tracker<K> {
    pub(crate) fn new(
        device: Arc<Device>,
        command_buffer_pools: K::CommandBufferPools,
    ) -> Result<Self, VulkanCommonError> {
        let semaphore_tracker = SemaphoreTracker::new(device)?;

        Ok(Self {
            semaphore_tracker,
            command_buffer_pools,
            image_layout_tracker: Default::default(),
        })
    }

    pub(crate) fn wait(&mut self, timeout: u64) -> Result<(), VulkanCommonError> {
        self.semaphore_tracker.wait(timeout)?;

        self.command_buffer_pools.mark_submitted_as_free();

        Ok(())
    }
}

pub(crate) struct SemaphoreTracker<S> {
    pub(crate) semaphore: TimelineSemaphore,
    next_value: u64,
    pub(crate) wait_for: Option<TrackerWait<S>>,
}

impl<S> SemaphoreTracker<S> {
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

#[derive(Debug, Default)]
pub(crate) struct ImageLayoutTracker {
    pub(crate) map: FxHashMap<ImageKey, Box<[vk::ImageLayout]>>,
}

impl ImageLayoutTracker {
    pub(crate) fn register_image(
        &mut self,
        image: ImageKey,
        initial_layout: vk::ImageLayout,
        array_layers: usize,
    ) -> Result<(), VulkanCommonError> {
        match self.map.entry(image) {
            Entry::Occupied(_) => Err(VulkanCommonError::RegisteredNewImageTwice(image)),
            Entry::Vacant(entry) => {
                entry.insert(vec![initial_layout; array_layers].into_boxed_slice());
                Ok(())
            }
        }
    }

    pub(crate) fn unregister_image(&mut self, image: ImageKey) -> Result<(), VulkanCommonError> {
        if self.map.remove(&image).is_none() {
            return Err(VulkanCommonError::UnregisteredNonexistentImage(image));
        }

        Ok(())
    }
}
