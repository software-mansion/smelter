use std::{
    collections::hash_map::Entry,
    sync::{Arc, Mutex, MutexGuard},
};

use ash::vk;
use rustc_hash::FxHashMap;

use crate::backends::vulkan::{VulkanCommonError, wrappers::ImageKey};

use super::Device;

pub(crate) struct TimelineSemaphore {
    pub(crate) semaphore: vk::Semaphore,
    device: Arc<Device>,
}

impl TimelineSemaphore {
    pub(crate) fn new(
        device: Arc<Device>,
        initial_value: u64,
        label: Option<&str>,
    ) -> Result<Self, VulkanCommonError> {
        let mut create_type_info = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value);
        let create_info = vk::SemaphoreCreateInfo::default().push_next(&mut create_type_info);
        let semaphore = unsafe { device.create_semaphore(&create_info, None)? };

        device.set_label(semaphore, label)?;

        Ok(Self { semaphore, device })
    }

    pub(crate) fn counter_value(&self) -> Result<u64, VulkanCommonError> {
        Ok(unsafe { self.device.get_semaphore_counter_value(self.semaphore)? })
    }

    pub(crate) fn wait(
        &self,
        timeout: u64,
        value: SemaphoreWaitValue,
    ) -> Result<(), VulkanCommonError> {
        let wait_info = vk::SemaphoreWaitInfo::default()
            .semaphores(std::slice::from_ref(&self.semaphore))
            .values(std::slice::from_ref(&value.0));

        unsafe { self.device.wait_semaphores(&wait_info, timeout)? };

        Ok(())
    }
}

impl Drop for TimelineSemaphore {
    fn drop(&mut self) {
        unsafe { self.device.destroy_semaphore(self.semaphore, None) };
    }
}

pub(crate) trait TrackerKind: Send {
    type WaitState: Send + Clone;
    type CommandBufferPools: CommandBufferPoolStorage + Send + Clone;
}

pub(crate) trait CommandBufferPoolStorage: Sized {
    fn mark_submitted_as_free(&self, last_waited_for: SemaphoreWaitValue);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SemaphoreWaitValue(pub(crate) u64);

pub(crate) struct TrackerWait<S> {
    pub(crate) value: SemaphoreWaitValue,
    pub(crate) _state: S,
}

impl<S: Clone> Clone for TrackerWait<S> {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            _state: self._state.clone(),
        }
    }
}

impl<S: Copy> Copy for TrackerWait<S> {}

pub(crate) struct Tracker<K: TrackerKind> {
    pub(crate) semaphore_tracker: Arc<SemaphoreTracker<K::WaitState>>,
    pub(crate) command_buffer_pools: K::CommandBufferPools,
    pub(crate) image_layout_tracker: Arc<Mutex<ImageLayoutTracker>>,
}

impl<K: TrackerKind> Clone for Tracker<K> {
    fn clone(&self) -> Self {
        Self {
            semaphore_tracker: self.semaphore_tracker.clone(),
            command_buffer_pools: self.command_buffer_pools.clone(),
            image_layout_tracker: self.image_layout_tracker.clone(),
        }
    }
}

impl<K: TrackerKind> Tracker<K> {
    pub(crate) fn new(
        device: Arc<Device>,
        command_buffer_pools: K::CommandBufferPools,
        label: Option<&str>,
    ) -> Result<Self, VulkanCommonError> {
        let semaphore_tracker = SemaphoreTracker::new(
            device,
            label.map(|name| format!("{name} semaphore")).as_deref(),
        )?;

        Ok(Self {
            semaphore_tracker: Arc::new(semaphore_tracker),
            command_buffer_pools,
            image_layout_tracker: Default::default(),
        })
    }

    pub(crate) fn wait_for_all(&self, timeout: u64) -> Result<(), VulkanCommonError> {
        let waited_for = self.semaphore_tracker.wait_for_all(timeout)?;

        if let Some(waited_for) = waited_for {
            self.mark_waited(waited_for);
        }

        Ok(())
    }

    pub(crate) fn wait_for(
        &self,
        value: SemaphoreWaitValue,
        timeout: u64,
    ) -> Result<(), VulkanCommonError> {
        self.semaphore_tracker.wait_for(value, timeout)?;
        self.mark_waited(value);
        Ok(())
    }

    /// Call this to mark that this value was waited for already
    pub(crate) fn mark_waited(&self, value: SemaphoreWaitValue) {
        self.command_buffer_pools.mark_submitted_as_free(value);
    }

    pub(crate) fn last_signaled_value(&self) -> Result<SemaphoreWaitValue, VulkanCommonError> {
        Ok(SemaphoreWaitValue(
            self.semaphore_tracker.semaphore.counter_value()?,
        ))
    }

    #[cfg_attr(not(feature = "wgpu"), allow(dead_code))]
    pub(crate) fn raw_semaphore(&self) -> vk::Semaphore {
        self.semaphore_tracker.semaphore.semaphore
    }
}

pub(crate) struct SemaphoreSubmitInfo<'a, S> {
    info: MutexGuard<'a, NextSubmissionInfo<S>>,
    new_state: S,
    tracker: &'a SemaphoreTracker<S>,

    #[cfg(feature = "wgpu")]
    wgpu_fence: wgpu::hal::vulkan::Fence,
}

impl<'a, S: Clone> SemaphoreSubmitInfo<'a, S> {
    pub(crate) fn wait_info(
        &self,
        stage: vk::PipelineStageFlags2,
    ) -> Option<vk::SemaphoreSubmitInfo<'_>> {
        self.info.wait_for.as_ref().map(|w| {
            vk::SemaphoreSubmitInfo::default()
                .stage_mask(stage)
                .value(w.value.0)
                .semaphore(self.tracker.semaphore.semaphore)
        })
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn wgpu_wait_info(&mut self) -> (&wgpu::hal::vulkan::Fence, u64) {
        (&self.wgpu_fence, self.info.signal_value.0)
    }

    pub(crate) fn signal_info(
        &self,
        stage: vk::PipelineStageFlags2,
    ) -> vk::SemaphoreSubmitInfo<'_> {
        vk::SemaphoreSubmitInfo::default()
            .stage_mask(stage)
            .value(self.info.signal_value.0)
            .semaphore(self.tracker.semaphore.semaphore)
    }

    pub(crate) fn signal_value(&self) -> SemaphoreWaitValue {
        self.info.signal_value
    }

    pub(crate) fn mark_submitted(mut self) {
        let wait_for = self.info.mark_submitted(self.new_state);
        self.tracker.update_wait_for(wait_for);
    }
}

pub(crate) struct NextSubmissionInfo<S> {
    pub(crate) signal_value: SemaphoreWaitValue,
    pub(crate) wait_for: Option<TrackerWait<S>>,
}

impl<S: Clone> NextSubmissionInfo<S> {
    fn new(next_value: SemaphoreWaitValue) -> Self {
        Self {
            signal_value: next_value,
            wait_for: None,
        }
    }

    fn mark_submitted(&mut self, new_state: S) -> TrackerWait<S> {
        let wait_for = TrackerWait {
            value: self.signal_value,
            _state: new_state,
        };
        self.wait_for = Some(wait_for.clone());
        self.signal_value = SemaphoreWaitValue(self.signal_value.0 + 1);

        wait_for
    }
}

pub(crate) struct SemaphoreWaitInfo<S> {
    wait_for: Option<TrackerWait<S>>,
    last_waited_for: Option<SemaphoreWaitValue>,
}

impl<S> Default for SemaphoreWaitInfo<S> {
    fn default() -> Self {
        Self {
            wait_for: None,
            last_waited_for: None,
        }
    }
}

pub(crate) struct SemaphoreTracker<S> {
    pub(crate) semaphore: TimelineSemaphore,
    wait_info: Mutex<SemaphoreWaitInfo<S>>,
    next_submission: Mutex<NextSubmissionInfo<S>>,
}

impl<S: Clone> SemaphoreTracker<S> {
    pub(crate) fn new(device: Arc<Device>, label: Option<&str>) -> Result<Self, VulkanCommonError> {
        Ok(Self {
            semaphore: TimelineSemaphore::new(device, 0, label)?,
            wait_info: Mutex::new(SemaphoreWaitInfo::default()),
            next_submission: Mutex::new(NextSubmissionInfo::new(SemaphoreWaitValue(1))),
        })
    }

    pub(crate) fn next_submit_info(&self, new_state: S) -> SemaphoreSubmitInfo<'_, S> {
        let next_submission = self.next_submission.lock().unwrap();

        SemaphoreSubmitInfo {
            info: next_submission,
            new_state,
            #[cfg(feature = "wgpu")]
            wgpu_fence: wgpu::hal::vulkan::Fence::TimelineSemaphore(self.semaphore.semaphore),
            tracker: self,
        }
    }

    /// This is a noop if there's nothing to wait for
    pub(crate) fn wait_for_all(
        &self,
        timeout: u64,
    ) -> Result<Option<SemaphoreWaitValue>, VulkanCommonError> {
        let wait_for = {
            let wait_info = self.wait_info.lock().unwrap();
            wait_info.wait_for.as_ref().map(|w| w.value)
        };

        if let Some(waited_for) = wait_for {
            self.semaphore.wait(timeout, waited_for)?;

            let mut wait_info = self.wait_info.lock().unwrap();
            if let Some(wait_for) = wait_info.wait_for.as_ref()
                && wait_for.value == waited_for
            {
                wait_info.wait_for = None;
            }

            match wait_info.last_waited_for {
                Some(old_value) => wait_info.last_waited_for = Some(old_value.max(waited_for)),
                None => wait_info.last_waited_for = Some(waited_for),
            }

            return Ok(Some(waited_for));
        }

        Ok(None)
    }

    pub(crate) fn wait_for(
        &self,
        value: SemaphoreWaitValue,
        timeout: u64,
    ) -> Result<(), VulkanCommonError> {
        {
            let wait_info = self.wait_info.lock().unwrap();
            if let Some(last) = wait_info.last_waited_for.as_ref()
                && *last >= value
            {
                return Ok(());
            }

            let Some(final_wait_for) = wait_info.wait_for.as_ref() else {
                return Err(VulkanCommonError::SemaphoreWaitOnUnsignaledValue);
            };

            if final_wait_for.value < value {
                return Err(VulkanCommonError::SemaphoreWaitOnUnsignaledValue);
            }
        }

        self.semaphore.wait(timeout, value)?;

        let mut wait_info = self.wait_info.lock().unwrap();
        if let Some(wait_for) = wait_info.wait_for.as_ref()
            && wait_for.value == value
        {
            wait_info.wait_for = None;
        }

        match wait_info.last_waited_for {
            Some(old_value) => wait_info.last_waited_for = Some(old_value.max(value)),
            None => wait_info.last_waited_for = Some(value),
        }

        Ok(())
    }

    fn update_wait_for(&self, wait_for: TrackerWait<S>) {
        self.wait_info.lock().unwrap().wait_for = Some(wait_for);
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
