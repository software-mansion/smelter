use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ShutdownCondition(VecDeque<Arc<AtomicBool>>);

impl Default for ShutdownCondition {
    fn default() -> Self {
        Self(VecDeque::from([Arc::new(AtomicBool::new(false))]))
    }
}

impl ShutdownCondition {
    /// Closing child condition will not close the parent, but
    /// closing parent will close child condition
    pub fn child_condition(&self) -> Self {
        let mut child = self.0.clone();
        child.push_back(Arc::new(AtomicBool::new(false)));
        Self(child)
    }

    pub fn mark_for_shutdown(&self) {
        self.0.back().unwrap().store(true, Ordering::Relaxed);
    }

    pub fn should_close(&self) -> bool {
        self.0.iter().any(|val| val.load(Ordering::Relaxed))
    }
}
