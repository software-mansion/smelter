use std::sync::{Arc, Mutex};

use super::{buffer::LiveSyncBuffer, state::SharedState};
use crate::pipeline::utils::input_sync::{TrackClosedError, TrackKind};

/// Write handle to one track of an input; a thin wrapper around the input's
/// shared state, which owns all state (per-track state included). Each
/// handle is independently owned, so tracks can be processed on separate
/// threads; every operation locks the input-wide state. Chunks leave the
/// track through the callback it was registered with.
pub(crate) struct LiveSyncTrack<B: LiveSyncBuffer> {
    shared: Arc<Mutex<SharedState<B>>>,
    kind: TrackKind,
}

impl<B: LiveSyncBuffer> LiveSyncTrack<B> {
    pub(super) fn new(shared: Arc<Mutex<SharedState<B>>>, kind: TrackKind) -> Self {
        Self { shared, kind }
    }

    pub fn write_chunk(&mut self, item: B::Chunk) -> Result<(), TrackClosedError> {
        self.shared.lock().unwrap().write_chunk(self.kind, item)
    }
}
