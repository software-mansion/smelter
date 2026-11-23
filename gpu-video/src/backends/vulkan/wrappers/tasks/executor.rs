use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::backends::vulkan::{vulkan_device::queues::QueueIndices, wrappers::Device};

use super::{
    ExecutionContext, Queue, QueueBinding, Role, Sharing, TaskError,
    waiter::{Request, SharedState, WaiterThreadHandle},
};

#[cfg(feature = "wgpu")]
use super::WgpuBinding;

pub(crate) struct Executor {
    pub(super) shared: Arc<SharedState>,
    pub(super) waiter: WaiterThreadHandle,
    pub(super) queues: Vec<Arc<Queue>>,
    pub(super) role_queues: HashMap<Role, Vec<Arc<Queue>>>,
    pub(super) next_queue: Mutex<HashMap<Role, usize>>,
    pub(super) submit_lock: Mutex<()>,
}

impl Executor {
    pub(crate) fn new(
        device: Arc<Device>,
        indices: &QueueIndices<'_>,
    ) -> Result<Arc<Self>, TaskError> {
        todo!()
    }

    pub(crate) fn bind(
        self: &Arc<Self>,
        role: Role,
        label: &str,
    ) -> Result<QueueBinding, TaskError> {
        todo!()
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn bind_wgpu(
        self: &Arc<Self>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<WgpuBinding, TaskError> {
        todo!()
    }

    pub(crate) fn execution_context(self: &Arc<Self>, max_in_flight: usize) -> ExecutionContext {
        todo!()
    }

    pub(crate) fn sharing(&self, roles: &[Role]) -> Result<Sharing, TaskError> {
        todo!()
    }

    pub(crate) fn family_index(&self, role: Role) -> Option<u32> {
        todo!()
    }

    pub(crate) fn supports_result_status_queries(&self, role: Role) -> bool {
        todo!()
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        todo!()
    }

    pub(crate) fn on_waiter_thread(&self) -> bool {
        todo!()
    }

    pub(crate) fn lost(&self) -> bool {
        todo!()
    }

    pub(super) fn lock_submit(&self) -> MutexGuard<'_, ()> {
        todo!()
    }

    pub(super) fn register(&self, request: Request) {
        todo!()
    }

    pub(super) fn mark_lost(&self) {
        todo!()
    }

    fn pick_queue(&self, role: Role) -> Result<Arc<Queue>, TaskError> {
        todo!()
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        todo!()
    }
}
