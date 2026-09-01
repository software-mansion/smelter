use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{BoxedTrackSink, InputSyncItem, TimestampAnchor, TrackClosedError, TrackEvent};

/// The most basic sync mechanism.
/// - Buffers at the start to have at least `desired_buffer` of data.
/// - Normalizes all PTS to start from zero (based on minimal not last pts)
pub(crate) struct SimpleSync {
    state: Arc<Mutex<SimpleSyncState>>,
}

struct SimpleSyncState {
    desired_buffer: Duration,
    /// Lowest pts seen so far; fixed once released.
    min_pts: Option<Duration>,
    /// Highest pts seen so far, used to measure the collected buffer.
    max_pts: Option<Duration>,
    buffering: bool,
}

impl SimpleSyncState {
    /// Records a written pts. Returns the anchor once the buffer is released.
    fn register_pts(&mut self, pts: Duration) -> Option<TimestampAnchor> {
        if !self.buffering {
            return Some(self.anchor());
        };
        let min_pts = self.min_pts.map_or(pts, |min| Duration::min(min, pts));
        let max_pts = self.max_pts.map_or(pts, |max| Duration::max(max, pts));
        self.min_pts = Some(min_pts);
        self.max_pts = Some(max_pts);

        if max_pts - min_pts >= self.desired_buffer {
            self.buffering = false;
        }

        match self.buffering {
            false => Some(self.anchor()),
            true => None,
        }
    }

    fn anchor(&self) -> TimestampAnchor {
        TimestampAnchor {
            input_pts: self.min_pts.unwrap_or(Duration::ZERO),
            output_pts: Duration::ZERO,
        }
    }
}

impl SimpleSync {
    pub fn new(desired_buffer: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(SimpleSyncState {
                desired_buffer,
                min_pts: None,
                max_pts: None,
                buffering: true,
            })),
        }
    }

    pub fn add_track<T: InputSyncItem>(&self, sink: BoxedTrackSink<T>) -> SimpleSyncTrack<T> {
        SimpleSyncTrack {
            state: self.state.clone(),
            anchor: None,
            buffer: Vec::new(),
            sink,
        }
    }

    /// Stop holding chunks back; each track pushes what it holds on its next
    /// write or when it is dropped.
    pub fn flush(&self) {
        self.state.lock().unwrap().buffering = false;
    }
}

pub(crate) struct SimpleSyncTrack<T: InputSyncItem> {
    state: Arc<Mutex<SimpleSyncState>>,
    /// Set once the shared state reports released. The anchor is fixed from
    /// then on, so the shared state does not have to be checked anymore.
    anchor: Option<TimestampAnchor>,
    /// Chunks with raw timestamps, held back until the anchor is known.
    buffer: Vec<T>,
    sink: BoxedTrackSink<T>,
}

impl<T: InputSyncItem> SimpleSyncTrack<T> {
    pub fn write_chunk(&mut self, mut chunk: T) -> Result<(), TrackClosedError> {
        if self.sink.is_closed() {
            return Err(TrackClosedError);
        }
        let anchor = match self.anchor {
            Some(anchor) => anchor,
            None => match self.state.lock().unwrap().register_pts(chunk.pts()) {
                Some(anchor) => {
                    self.anchor = Some(anchor);
                    anchor
                }
                None => {
                    self.buffer.push(chunk);
                    return Ok(());
                }
            },
        };
        for mut buffered in self.buffer.drain(..) {
            buffered.apply_anchor(anchor);
            self.sink.on_event(TrackEvent::Chunk(buffered));
        }
        chunk.apply_anchor(anchor);
        self.sink.on_event(TrackEvent::Chunk(chunk));
        Ok(())
    }
}

impl<T: InputSyncItem> Drop for SimpleSyncTrack<T> {
    fn drop(&mut self) {
        // the stream can end before the desired buffer is collected
        if self.sink.is_closed() {
            return;
        }
        let anchor = self
            .anchor
            .unwrap_or_else(|| self.state.lock().unwrap().anchor());
        for mut chunk in self.buffer.drain(..) {
            chunk.apply_anchor(anchor);
            self.sink.on_event(TrackEvent::Chunk(chunk));
        }
    }
}
