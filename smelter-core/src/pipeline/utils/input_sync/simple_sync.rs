use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use super::InputSyncItem;

/// Synchronization for non-live inputs: normalizes timestamps of all tracks
/// to start at zero, based on the first chunk written to any track. Chunks
/// are never held back; a chunk can be read as soon as it is written.
pub(crate) struct SimpleSync {
    first_pts: Arc<Mutex<Option<Duration>>>,
}

impl SimpleSync {
    pub fn new() -> Self {
        Self {
            first_pts: Arc::new(Mutex::new(None)),
        }
    }

    pub fn add_track<T: InputSyncItem>(&self) -> SimpleSyncTrack<T> {
        SimpleSyncTrack {
            first_pts: self.first_pts.clone(),
            buffer: VecDeque::new(),
        }
    }

}

pub(crate) struct SimpleSyncTrack<T: InputSyncItem> {
    first_pts: Arc<Mutex<Option<Duration>>>,
    buffer: VecDeque<T>,
}

impl<T: InputSyncItem> SimpleSyncTrack<T> {
    pub fn write_chunk(&mut self, item: T) {
        self.first_pts.lock().unwrap().get_or_insert(item.pts());
        self.buffer.push_back(item);
    }

    pub fn try_read_chunk(&mut self) -> Option<T> {
        let mut item = self.buffer.pop_front()?;
        let first_pts = self.first_pts.lock().unwrap().unwrap_or(Duration::ZERO);
        item.map_timestamps(|pts| pts.saturating_sub(first_pts));
        Some(item)
    }

    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        self.buffer.front().map(|item| item.pts())
    }

    pub fn has_buffered_chunks(&self) -> bool {
        !self.buffer.is_empty()
    }
}
