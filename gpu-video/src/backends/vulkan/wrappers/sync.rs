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

    pub(crate) fn counter_value(&self) -> Result<SemaphoreWaitValue, VulkanCommonError> {
        let value = unsafe { self.device.get_semaphore_counter_value(self.semaphore)? };
        Ok(SemaphoreWaitValue(value))
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

    pub(crate) fn signal(&self, value: SemaphoreWaitValue) -> Result<(), VulkanCommonError> {
        let signal_info = vk::SemaphoreSignalInfo::default()
            .semaphore(self.semaphore)
            .value(value.0);

        unsafe { self.device.signal_semaphore(&signal_info)? };

        Ok(())
    }
}

impl Drop for TimelineSemaphore {
    fn drop(&mut self) {
        unsafe { self.device.destroy_semaphore(self.semaphore, None) };
    }
}

pub(crate) trait TrackerKind: Send {
    type WaitState: Send;
    type CommandBufferPools: CommandBufferPoolStorage + Send;
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
        self.semaphore_tracker.semaphore.counter_value()
    }

    #[cfg_attr(not(feature = "wgpu"), allow(dead_code))]
    pub(crate) fn raw_semaphore(&self) -> vk::Semaphore {
        self.semaphore_tracker.semaphore.semaphore
    }
}

pub(crate) struct SemaphoreSubmitInfo<'a, S> {
    state: MutexGuard<'a, SemaphoreState<S>>,
    new_state: S,
    semaphore: vk::Semaphore,

    #[cfg(feature = "wgpu")]
    wgpu_fence: wgpu::hal::vulkan::Fence,
}

impl<'a, S> SemaphoreSubmitInfo<'a, S> {
    pub(crate) fn wait_info(
        &self,
        stage: vk::PipelineStageFlags2,
    ) -> Option<vk::SemaphoreSubmitInfo<'_>> {
        self.state.wait_for.as_ref().map(|w| {
            vk::SemaphoreSubmitInfo::default()
                .stage_mask(stage)
                .value(w.value.0)
                .semaphore(self.semaphore)
        })
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn wgpu_wait_info(&mut self) -> (&wgpu::hal::vulkan::Fence, u64) {
        (&self.wgpu_fence, self.state.signal_value.0)
    }

    pub(crate) fn signal_info(
        &self,
        stage: vk::PipelineStageFlags2,
    ) -> vk::SemaphoreSubmitInfo<'_> {
        vk::SemaphoreSubmitInfo::default()
            .stage_mask(stage)
            .value(self.state.signal_value.0)
            .semaphore(self.semaphore)
    }

    pub(crate) fn signal_value(&self) -> SemaphoreWaitValue {
        self.state.signal_value
    }

    pub(crate) fn mark_submitted(mut self) {
        self.state.mark_submitted(self.new_state)
    }
}

struct SemaphoreState<S> {
    signal_value: SemaphoreWaitValue,
    wait_for: Option<TrackerWait<S>>,
    last_waited_for: Option<SemaphoreWaitValue>,
}

impl<S> SemaphoreState<S> {
    fn mark_waited(&mut self, value: SemaphoreWaitValue) {
        if let Some(wait_for) = self.wait_for.as_ref()
            && wait_for.value == value
        {
            self.wait_for = None;
        }

        match self.last_waited_for {
            Some(old_value) => self.last_waited_for = Some(old_value.max(value)),
            None => self.last_waited_for = Some(value),
        }
    }

    pub(crate) fn mark_submitted(&mut self, new_state: S) {
        self.wait_for = Some(TrackerWait {
            value: self.signal_value,
            _state: new_state,
        });
        self.signal_value = SemaphoreWaitValue(self.signal_value.0 + 1);
    }
}

pub(crate) struct SemaphoreTracker<S> {
    pub(crate) semaphore: Arc<TimelineSemaphore>,
    state: Mutex<SemaphoreState<S>>,
}

impl<S> SemaphoreTracker<S> {
    pub(crate) fn new(device: Arc<Device>, label: Option<&str>) -> Result<Self, VulkanCommonError> {
        Ok(Self {
            semaphore: Arc::new(TimelineSemaphore::new(device, 0, label)?),
            state: Mutex::new(SemaphoreState {
                signal_value: SemaphoreWaitValue(1),
                wait_for: None,
                last_waited_for: None,
            }),
        })
    }

    pub(crate) fn next_submit_info(&self, new_state: S) -> SemaphoreSubmitInfo<'_, S> {
        SemaphoreSubmitInfo {
            state: self.state.lock().unwrap(),
            new_state,
            #[cfg(feature = "wgpu")]
            wgpu_fence: wgpu::hal::vulkan::Fence::TimelineSemaphore(self.semaphore.semaphore),
            semaphore: self.semaphore.semaphore,
        }
    }

    /// This is a noop if there's nothing to wait for
    pub(crate) fn wait_for_all(
        &self,
        timeout: u64,
    ) -> Result<Option<SemaphoreWaitValue>, VulkanCommonError> {
        let wait_for = {
            let state = self.state.lock().unwrap();
            state.wait_for.as_ref().map(|w| w.value)
        };

        if let Some(waited_for) = wait_for {
            self.semaphore.wait(timeout, waited_for)?;
            self.state.lock().unwrap().mark_waited(waited_for);
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
            let state = self.state.lock().unwrap();
            if let Some(last) = state.last_waited_for.as_ref()
                && *last >= value
            {
                return Ok(());
            }

            let Some(final_wait_for) = state.wait_for.as_ref() else {
                return Err(VulkanCommonError::SemaphoreWaitOnUnsignaledValue);
            };

            if final_wait_for.value < value {
                return Err(VulkanCommonError::SemaphoreWaitOnUnsignaledValue);
            }
        }

        self.semaphore.wait(timeout, value)?;
        self.state.lock().unwrap().mark_waited(value);

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
