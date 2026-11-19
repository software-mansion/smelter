use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU64},
};

use ash::vk;

use crate::backends::vulkan::wrappers::{Device, TimelineSemaphore};

use super::{CommandBufferPool, Executor, GraphError};

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
    pub(super) fn new(device: Arc<Device>, label: &str) -> Result<Self, GraphError> {
        todo!()
    }

    pub(crate) fn raw(&self) -> vk::Semaphore {
        todo!()
    }

    pub(crate) fn completed(&self) -> u64 {
        todo!()
    }

    pub(crate) fn refresh(&self) -> Result<u64, vk::Result> {
        todo!()
    }

    pub(super) fn store_completed(&self, value: u64) {
        todo!()
    }

    pub(super) fn state(&self) -> MutexGuard<'_, TimelineState> {
        todo!()
    }

    pub(super) fn mark_pinned(&self) -> bool {
        todo!()
    }
}

impl TimelineState {
    pub(super) fn reserve(&mut self) -> u64 {
        todo!()
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
        todo!()
    }

    pub(crate) fn family_index(&self) -> u32 {
        todo!()
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
    pub(crate) fn check_same_device(&self, device: &wgpu::Device) -> Result<(), GraphError> {
        todo!()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sharing {
    pub(super) mode: vk::SharingMode,
    pub(super) families: Vec<u32>,
}

impl Sharing {
    pub(super) fn from_families(families: impl IntoIterator<Item = u32>) -> Self {
        todo!()
    }

    pub(crate) fn is_exclusive(&self) -> bool {
        todo!()
    }

    pub(crate) fn families(&self) -> &[u32] {
        todo!()
    }

    pub(crate) fn allows_family(&self, family: u32, owner: Option<u32>) -> bool {
        todo!()
    }
}
