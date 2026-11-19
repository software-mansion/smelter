use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::Receiver},
    time::Duration,
};

use super::{Completion, ExecError, Executor, Graph, GraphError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WindowId(pub(super) u64);

pub(crate) struct InFlightWindow {
    pub(super) executor: Arc<Executor>,
    pub(super) shared: Arc<WindowShared>,
    pub(super) max_in_flight: usize,
    pub(super) in_flight: VecDeque<Receiver<Result<(), ExecError>>>,
}

pub(crate) struct WindowShared {
    pub(super) id: WindowId,
    pub(super) open: AtomicBool,
    pub(super) poisoned: Mutex<Option<GraphError>>,
}

impl WindowShared {
    pub(super) fn new() -> Self {
        todo!()
    }

    pub(super) fn poison(&self, error: GraphError) {
        todo!()
    }

    pub(super) fn take_poison(&self) -> Option<GraphError> {
        todo!()
    }
}

impl InFlightWindow {
    pub(super) fn new(executor: Arc<Executor>, max_in_flight: usize) -> Self {
        todo!()
    }

    pub(crate) fn frame(&mut self) -> Result<Graph, GraphError> {
        todo!()
    }

    pub(crate) fn submit(&mut self, g: Graph) -> Result<Completion, GraphError> {
        todo!()
    }

    pub(crate) fn wait_idle(&mut self, timeout: Duration) -> Result<(), GraphError> {
        todo!()
    }

    pub(crate) fn in_flight(&self) -> usize {
        todo!()
    }

    fn wait_for_slot(&mut self) -> Result<(), GraphError> {
        todo!()
    }

    fn receive_one(&mut self, timeout: Duration) -> Result<(), GraphError> {
        todo!()
    }

    fn apply_outcome(&self, outcome: Result<Result<(), ExecError>, std::sync::mpsc::RecvError>) {
        todo!()
    }

    fn check_phase_zero(&self, g: &Graph) -> Result<(), GraphError> {
        todo!()
    }

    fn submit_locked(&mut self, g: &mut Graph) -> Result<SubmittedGraph, GraphError> {
        todo!()
    }
}

pub(super) struct SubmittedGraph {
    pub(super) completion: Completion,
    pub(super) receiver: Option<Receiver<Result<(), ExecError>>>,
    pub(super) error: Option<GraphError>,
}

pub(super) struct SubmitPanicGuard<'a> {
    pub(super) window: &'a WindowShared,
}

impl Drop for SubmitPanicGuard<'_> {
    fn drop(&mut self) {
        todo!()
    }
}
