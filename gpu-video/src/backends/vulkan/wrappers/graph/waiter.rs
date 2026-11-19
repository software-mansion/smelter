use std::{
    sync::{Arc, Condvar, Mutex, atomic::AtomicBool, mpsc::Sender},
    thread::{JoinHandle, ThreadId},
};

use crate::backends::vulkan::wrappers::{Device, TimelineSemaphore};

use super::{CompletionCallback, ExecError, GraphError, Retired, Timeline, WindowId};

pub(super) struct SharedState {
    pub(super) device: Arc<Device>,
    pub(super) waker: Waker,
    pub(super) requests: Mutex<Vec<Request>>,
    pub(super) requests_changed: Condvar,
    pub(super) should_quit: AtomicBool,
    pub(super) lost: AtomicBool,
    pub(super) died: AtomicBool,
    pub(super) wgpu_pinned: Mutex<Vec<Arc<Timeline>>>,
}

impl SharedState {
    pub(super) fn new(device: Arc<Device>) -> Result<Self, GraphError> {
        todo!()
    }

    pub(super) fn push_request(&self, request: Request) {
        todo!()
    }

    pub(super) fn pin(&self, timeline: &Arc<Timeline>) {
        todo!()
    }

    pub(super) fn is_lost(&self) -> bool {
        todo!()
    }

    pub(super) fn mark_lost(&self) {
        todo!()
    }

    pub(super) fn has_died(&self) -> bool {
        todo!()
    }
}

pub(super) struct Request {
    pub(super) waits: Vec<(Arc<Timeline>, u64)>,
    pub(super) window: WindowId,
    pub(super) retired: Option<Retired>,
    pub(super) on_complete: Option<CompletionCallback>,
    pub(super) outcome: Result<(), ExecError>,
    pub(super) sender: Sender<Result<(), ExecError>>,
}

impl Request {
    fn is_reached(&self) -> bool {
        todo!()
    }

    fn finish(self, lost: bool) {
        todo!()
    }
}

pub(super) struct Waker {
    wait_for: Mutex<u64>,
    semaphore: TimelineSemaphore,
}

impl Waker {
    fn new(device: Arc<Device>) -> Result<Self, GraphError> {
        todo!()
    }

    pub(super) fn wait_value(&self) -> u64 {
        todo!()
    }

    pub(super) fn wake(&self) {
        todo!()
    }
}

pub(super) struct WaiterThreadHandle {
    pub(super) handle: Option<JoinHandle<()>>,
    pub(super) thread_id: ThreadId,
}

impl WaiterThreadHandle {
    pub(super) fn spawn(shared: Arc<SharedState>) -> Result<Self, GraphError> {
        todo!()
    }

    pub(super) fn is_current_thread(&self) -> bool {
        todo!()
    }

    pub(super) fn shut_down(&mut self, shared: &SharedState) {
        todo!()
    }
}

struct WaiterThread {
    shared: Arc<SharedState>,
}

struct DeathGuard {
    shared: Arc<SharedState>,
}

impl Drop for DeathGuard {
    fn drop(&mut self) {
        todo!()
    }
}

impl WaiterThread {
    fn run(self) {
        todo!()
    }

    fn run_normal_mode(&self) -> LoopState {
        todo!()
    }

    fn snapshot_heads(&self) -> Vec<Vec<(Arc<Timeline>, u64)>> {
        todo!()
    }

    fn refresh_counters(
        &self,
        heads: &[Vec<(Arc<Timeline>, u64)>],
    ) -> Result<bool, ash::vk::Result> {
        todo!()
    }

    fn finish_ready_fronts(&self) -> bool {
        todo!()
    }

    fn wait_any(&self, heads: &[Vec<(Arc<Timeline>, u64)>]) -> Result<(), ash::vk::Result> {
        todo!()
    }

    fn run_lost_mode(&self) {
        todo!()
    }

    fn back_off(&self) {
        todo!()
    }
}

enum LoopState {
    Continue,
    Lost,
    Quit,
}
