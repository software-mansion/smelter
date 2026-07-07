use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{InputSyncItem, TimestampAnchor, TrackCallback};

/// Synchronization for non-live inputs: normalizes timestamps of all tracks
/// to start at zero, based on the first chunk written to any track. Chunks
/// are never held back; every written chunk is pushed to the track callback
/// right away.
pub(crate) struct SimpleSync {
    first_pts: Arc<Mutex<Option<Duration>>>,
}

impl SimpleSync {
    pub fn new() -> Self {
        Self {
            first_pts: Arc::new(Mutex::new(None)),
        }
    }

    pub fn add_track<T: InputSyncItem>(&self, callback: TrackCallback<T>) -> SimpleSyncTrack<T> {
        SimpleSyncTrack {
            first_pts: self.first_pts.clone(),
            callback,
        }
    }
}

pub(crate) struct SimpleSyncTrack<T: InputSyncItem> {
    first_pts: Arc<Mutex<Option<Duration>>>,
    callback: TrackCallback<T>,
}

impl<T: InputSyncItem> SimpleSyncTrack<T> {
    pub fn write_chunk(&mut self, mut item: T) {
        // the callback may block, so the lock is released first
        let first_pts = *self.first_pts.lock().unwrap().get_or_insert(item.pts());
        item.apply_anchor(TimestampAnchor {
            input_pts: first_pts,
            output_pts: Duration::ZERO,
        });
        (self.callback)(item);
    }
}
