use std::sync::{
    Arc, Mutex, MutexGuard, PoisonError,
    atomic::{AtomicBool, AtomicU64},
};

use ash::vk;

use crate::backends::vulkan::wrappers::{Device, TimelineSemaphore};

use super::{CommandBufferPool, Executor, TaskError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Role {
    Decode,
    Encode,
    Transfer,
    Compute,
    Wgpu,
}

pub(crate) struct Queue {
    pub(super) raw: Mutex<vk::Queue>,
    pub(crate) family_index: u32,
    pub(crate) queue_index: u32,
    pub(crate) flags: vk::QueueFlags,
    pub(crate) video_properties: vk::QueueFamilyVideoPropertiesKHR<'static>,
    pub(crate) query_result_status_properties:
        vk::QueueFamilyQueryResultStatusPropertiesKHR<'static>,
}

impl Queue {
    pub(super) fn new(
        device: &Device,
        family_index: u32,
        queue_index: u32,
        flags: vk::QueueFlags,
        video_properties: vk::QueueFamilyVideoPropertiesKHR<'static>,
        query_result_status_properties: vk::QueueFamilyQueryResultStatusPropertiesKHR<'static>,
    ) -> Self {
        todo!()
    }

    pub(crate) fn supports_result_status_queries(&self) -> bool {
        todo!()
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, vk::Queue> {
        todo!()
    }
}

pub(crate) struct Timeline {
    pub(super) semaphore: TimelineSemaphore,
    pub(super) state: Mutex<TimelineState>,
    pub(super) completed: AtomicU64,
    pub(super) pinned: AtomicBool,
}

pub(super) struct TimelineState {
    pub(super) next_value: u64,
    pub(super) last_submitted: Option<u64>,
}

impl Timeline {
    pub(super) fn new(device: Arc<Device>, label: &str) -> Result<Self, TaskError> {
        let semaphore = TimelineSemaphore::new(device, 0, Some(label))?;
        Ok(Self {
            semaphore,
            state: TimelineState {
                next_value: 1,
                last_submitted: None,
            }
            .into(),
            completed: 0.into(),
            pinned: false.into(),
        })
    }

    pub(crate) fn raw(&self) -> vk::Semaphore {
        self.semaphore.semaphore
    }

    pub(crate) fn completed(&self) -> u64 {
        self.completed.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn refresh(&self) -> Result<u64, vk::Result> {
        let val = unsafe {
            self.semaphore
                .device
                .device
                .get_semaphore_counter_value(self.semaphore.semaphore)?
        };
        self.store_completed(val);
        Ok(val)
    }

    pub(super) fn store_completed(&self, value: u64) {
        self.completed
            .fetch_max(value, std::sync::atomic::Ordering::Release);
    }

    pub(super) fn state(&self) -> MutexGuard<'_, TimelineState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn mark_pinned(&self) -> bool {
        !self.pinned.swap(true, std::sync::atomic::Ordering::Relaxed)
    }
}

impl TimelineState {
    pub(super) fn reserve(&mut self) -> u64 {
        let val = self.next_value;
        self.next_value += 1;
        val
    }
}

pub(crate) struct QueueBinding {
    pub(super) role: Role,
    pub(super) queue: Arc<Queue>,
    pub(super) timeline: Arc<Timeline>,
    pub(super) pool: Arc<CommandBufferPool>,
    pub(super) executor: Arc<Executor>,
}

impl QueueBinding {
    pub(crate) fn role(&self) -> Role {
        self.role
    }

    pub(crate) fn family_index(&self) -> u32 {
        self.queue.family_index
    }
}

#[cfg(feature = "wgpu")]
pub(crate) struct WgpuBinding {
    pub(super) timeline: Arc<Timeline>,
    pub(super) probe: wgpu::Buffer,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) executor: Arc<Executor>,
}

#[cfg(feature = "wgpu")]
impl WgpuBinding {
    pub(crate) fn check_same_device(&self, device: &wgpu::Device) -> Result<(), TaskError> {
        todo!()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sharing {
    pub(super) mode: vk::SharingMode,
    pub(super) families: Vec<u32>,
}

impl Sharing {
    // TODO: temporary pub(crate), should go back to pub(super) at some point
    pub(crate) fn from_families(families: impl IntoIterator<Item = u32>) -> Self {
        let mut families = families.into_iter().collect::<Vec<_>>();
        families.sort_unstable();
        families.dedup();

        if families.len() <= 1 {
            Self {
                mode: vk::SharingMode::EXCLUSIVE,
                families: Vec::new(),
            }
        } else {
            Self {
                mode: vk::SharingMode::CONCURRENT,
                families: families.into_iter().collect(),
            }
        }
    }

    // TODO: temporary, will be removed later
    pub(crate) fn new_exclusive() -> Self {
        Self {
            mode: vk::SharingMode::EXCLUSIVE,
            families: Vec::new(),
        }
    }

    pub(crate) fn is_exclusive(&self) -> bool {
        self.mode == vk::SharingMode::EXCLUSIVE
    }

    pub(crate) fn families(&self) -> &[u32] {
        &self.families
    }

    pub(crate) fn allows_family(&self, family: u32, owner: Option<u32>) -> bool {
        match self.mode {
            vk::SharingMode::CONCURRENT => self.families.contains(&family),
            vk::SharingMode::EXCLUSIVE => owner.is_none_or(|owner| owner == family),
            _ => unreachable!(),
        }
    }
}
