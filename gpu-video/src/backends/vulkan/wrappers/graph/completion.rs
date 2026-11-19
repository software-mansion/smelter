use std::{any::Any, sync::Arc, time::Duration};

use ash::vk;

use crate::backends::vulkan::wrappers::ResultQueryData;

use super::{BufferRange, CommandBufferPool, GraphError, GraphId, HostRead, QueryRef, Timeline};

#[cfg(feature = "wgpu")]
use crate::backends::vulkan::wrappers::Image;

pub(crate) type Outcome<'a> = Result<&'a Retired, ExecError>;

#[derive(Clone, Copy, Debug)]
pub(crate) enum ExecError {
    PartialSubmit,
    DeviceLost,
    CallbackPanicked,
}

#[derive(Clone)]
pub(crate) struct Completion {
    pub(super) graph: GraphId,
    pub(super) waits: Vec<(Arc<Timeline>, u64)>,
}

impl Completion {
    pub(crate) fn is_complete(&self) -> Result<bool, GraphError> {
        todo!()
    }

    pub(crate) fn wait(&self, timeout: Duration) -> Result<(), GraphError> {
        todo!()
    }
}

pub(crate) struct Retired {
    pub(super) graph: GraphId,
    pub(super) command_buffers: Vec<(Arc<CommandBufferPool>, vk::CommandBuffer)>,
    pub(super) timelines: Vec<Arc<Timeline>>,
    pub(super) declared: Vec<Box<dyn Any + Send + Sync>>,
    pub(super) keep_alive: Vec<Box<dyn Any + Send>>,
}

impl Retired {
    pub(crate) fn read(&self, read: &HostRead<BufferRange>) -> Result<Vec<u8>, GraphError> {
        todo!()
    }

    pub(crate) fn read_at(
        &self,
        read: &HostRead<BufferRange>,
        offset: usize,
        len: usize,
    ) -> Result<Vec<u8>, GraphError> {
        todo!()
    }

    pub(crate) fn query<T: ResultQueryData + Send + Sync + 'static>(
        &self,
        read: &HostRead<QueryRef<T>>,
    ) -> Result<T, GraphError> {
        todo!()
    }

    pub(crate) fn query_status(
        &self,
        read: &HostRead<QueryRef<vk::QueryResultStatusKHR>>,
    ) -> Result<(), GraphError> {
        todo!()
    }

    fn check_declared(&self, graph: GraphId) -> Result<(), GraphError> {
        todo!()
    }
}

impl Drop for Retired {
    fn drop(&mut self) {
        todo!()
    }
}

#[cfg(feature = "wgpu")]
pub(crate) struct ExportedImage {
    pub(super) graph: GraphId,
    pub(crate) image: Arc<Image>,
    pub(crate) usage: wgpu::TextureUses,
    pub(crate) format: wgpu::TextureFormat,
    pub(super) keep: (Arc<Timeline>, Arc<Image>),
}

#[cfg(feature = "wgpu")]
impl ExportedImage {
    pub(crate) fn into_texture(
        self,
        submitted: &Completion,
        wgpu_device: &wgpu::Device,
        label: &str,
        usage: wgpu::TextureUsages,
    ) -> Result<wgpu::Texture, GraphError> {
        todo!()
    }
}
