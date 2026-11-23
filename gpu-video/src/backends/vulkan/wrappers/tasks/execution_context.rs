use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::Receiver},
    time::Duration,
};

use super::{Completion, ExecError, Executor, Task, TaskError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExecutionContextId(pub(super) u64);

pub(crate) struct ExecutionContext {
    pub(super) executor: Arc<Executor>,
    pub(super) shared: Arc<ExecutionContextShared>,
    pub(super) max_in_flight: usize,
    pub(super) in_flight: VecDeque<Receiver<Result<(), ExecError>>>,
}

pub(crate) struct ExecutionContextShared {
    pub(super) id: ExecutionContextId,
    pub(super) open: AtomicBool,
    pub(super) poisoned: Mutex<Option<TaskError>>,
}

impl ExecutionContextShared {
    pub(super) fn new() -> Self {
        todo!()
    }

    pub(super) fn poison(&self, error: TaskError) {
        todo!()
    }

    pub(super) fn take_poison(&self) -> Option<TaskError> {
        todo!()
    }
}

impl ExecutionContext {
    pub(super) fn new(executor: Arc<Executor>, max_in_flight: usize) -> Self {
        todo!()
    }

    pub(crate) fn open_task(&mut self) -> Result<Task, TaskError> {
        todo!()
    }

    pub(crate) fn submit(&mut self, g: Task) -> Result<Completion, TaskError> {
        todo!()
    }

    pub(crate) fn wait_idle(&mut self, timeout: Duration) -> Result<(), TaskError> {
        todo!()
    }

    pub(crate) fn in_flight(&self) -> usize {
        todo!()
    }

    fn wait_for_slot(&mut self) -> Result<(), TaskError> {
        todo!()
    }

    fn receive_one(&mut self, timeout: Duration) -> Result<(), TaskError> {
        todo!()
    }

    fn apply_outcome(&self, outcome: Result<Result<(), ExecError>, std::sync::mpsc::RecvError>) {
        todo!()
    }

    fn check_phase_zero(&self, g: &Task) -> Result<(), TaskError> {
        todo!()
    }

    fn submit_locked(&mut self, g: &mut Task) -> Result<SubmittedTask, TaskError> {
        todo!()
    }
}

pub(super) struct SubmittedTask {
    pub(super) completion: Completion,
    pub(super) receiver: Option<Receiver<Result<(), ExecError>>>,
    pub(super) error: Option<TaskError>,
}

pub(super) struct SubmitPanicGuard<'a> {
    pub(super) context: &'a ExecutionContextShared,
}

impl Drop for SubmitPanicGuard<'_> {
    fn drop(&mut self) {
        todo!()
    }
}
