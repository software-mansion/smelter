use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> Instant;

    fn elapsed_since(&self, since: Instant) -> Duration {
        self.now().saturating_duration_since(since)
    }
}

#[derive(Debug, Default)]
pub(crate) struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub(crate) type SharedClock = Arc<dyn Clock>;
