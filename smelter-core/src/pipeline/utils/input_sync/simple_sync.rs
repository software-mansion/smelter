use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{BoxedTrackSink, InputSyncItem, TimestampAnchor, TrackClosedError, TrackEvent};

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

    pub fn add_track<T: InputSyncItem>(&self, sink: BoxedTrackSink<T>) -> SimpleSyncTrack<T> {
        SimpleSyncTrack {
            first_pts: self.first_pts.clone(),
            sink,
        }
    }
}

pub(crate) struct SimpleSyncTrack<T: InputSyncItem> {
    first_pts: Arc<Mutex<Option<Duration>>>,
    sink: BoxedTrackSink<T>,
}

impl<T: InputSyncItem> SimpleSyncTrack<T> {
    pub fn write_chunk(&mut self, mut item: T) -> Result<(), TrackClosedError> {
        if self.sink.is_closed() {
            return Err(TrackClosedError);
        }
        // the sink may block, so the lock is released first
        let first_pts = *self.first_pts.lock().unwrap().get_or_insert(item.pts());
        item.apply_anchor(TimestampAnchor {
            input_pts: first_pts,
            output_pts: Duration::ZERO,
        });
        self.sink.on_event(TrackEvent::Chunk(item));
        Ok(())
    }
}
