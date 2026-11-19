use std::sync::{Arc, Mutex};

use ash::vk;

use crate::backends::vulkan::wrappers::Device;

use super::GraphError;

pub(crate) struct CommandBufferPool {
    inner: Mutex<CommandBufferPoolInner>,
}

struct CommandBufferPoolInner {
    command_pool: vk::CommandPool,
    free: Vec<vk::CommandBuffer>,
    device: Arc<Device>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReleasedState {
    Executable,
    Invalid,
}

impl CommandBufferPool {
    pub(crate) fn new(device: Arc<Device>, family_index: u32) -> Result<Self, GraphError> {
        todo!()
    }

    pub(crate) fn begin(&self) -> Result<vk::CommandBuffer, GraphError> {
        todo!()
    }

    pub(crate) fn release(&self, command_buffer: vk::CommandBuffer, state: ReleasedState) {
        todo!()
    }

    pub(super) fn recording(self: &Arc<Self>) -> Result<RecordingGuard, GraphError> {
        todo!()
    }
}

impl Drop for CommandBufferPool {
    fn drop(&mut self) {
        todo!()
    }
}

pub(crate) struct RecordingGuard {
    pool: Arc<CommandBufferPool>,
    command_buffer: vk::CommandBuffer,
    finished: bool,
}

impl RecordingGuard {
    pub(super) fn command_buffer(&self) -> vk::CommandBuffer {
        todo!()
    }

    pub(super) fn end(self) -> Result<vk::CommandBuffer, GraphError> {
        todo!()
    }
}

impl Drop for RecordingGuard {
    fn drop(&mut self) {
        todo!()
    }
}
