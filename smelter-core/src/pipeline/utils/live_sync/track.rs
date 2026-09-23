use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use super::{buffer::LiveSyncBuffer, state::SharedState};
use crate::{
    Timestamp,
    pipeline::utils::input_sync::{TrackClosedError, TrackKind},
};

/// Write handle to one track of an input; a thin wrapper around the input's
/// shared state, which owns all state (per-track state included). Each
/// handle is independently owned, so tracks can be processed on separate
/// threads; every operation locks the input-wide state. Chunks leave the
/// track through the callback it was registered with.
pub(crate) struct LiveSyncTrack<B: LiveSyncBuffer> {
    shared: Arc<Mutex<SharedState<B>>>,
    kind: TrackKind,
    deadline: LiveSyncDeadline,
}

impl<B: LiveSyncBuffer> LiveSyncTrack<B> {
    pub(super) fn new(
        shared: Arc<Mutex<SharedState<B>>>,
        kind: TrackKind,
        deadline: LiveSyncDeadline,
    ) -> Self {
        Self {
            shared,
            kind,
            deadline,
        }
    }

    pub fn write_chunk(&mut self, item: B::Chunk) -> Result<(), TrackClosedError> {
        self.shared.lock().unwrap().write_chunk(self.kind, item)
    }

    pub fn deadline(&self) -> LiveSyncDeadline {
        self.deadline.clone()
    }
}

/// Oldest input pts the track can still present. Lets the input stop reading content that has
/// no chance of playing without locking the sync state. `None` while the track has no anchor.
#[derive(Clone)]
pub(crate) struct LiveSyncDeadline(Arc<AtomicI64>);

impl LiveSyncDeadline {
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicI64::new(Timestamp::MIN.as_nanos())))
    }

    pub fn get(&self) -> Option<Timestamp> {
        match Timestamp::from_nanos(self.0.load(Ordering::Relaxed)) {
            Timestamp::MIN => None,
            deadline => Some(deadline),
        }
    }

    pub(super) fn set(&self, deadline: Option<Timestamp>) {
        let deadline = deadline.unwrap_or(Timestamp::MIN);
        self.0.store(deadline.as_nanos(), Ordering::Relaxed);
    }
}
