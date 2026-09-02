use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{
    BoxedTrackSink, InputSyncItem, InputSyncStatsSender, TimestampAnchor, TrackClosedError,
    TrackEvent, TrackKind,
};
use crate::{
    Timestamp,
    stats::{InputSyncMode, InputSyncTrackStatsEvent, SimpleSyncStatsEvent, SimpleSyncTrackState},
};

/// The most basic sync mechanism.
/// - Buffers at the start to have at least `desired_buffer` of data.
/// - Normalizes all PTS to start from zero (based on minimal not last pts)
pub(crate) struct SimpleSync {
    state: Arc<Mutex<SimpleSyncState>>,
    stats: InputSyncStatsSender,
}

struct SimpleSyncState {
    desired_buffer: Duration,
    /// Lowest pts seen so far; fixed once released.
    min_pts: Option<Timestamp>,
    /// Highest pts seen so far, used to measure the collected buffer.
    max_pts: Option<Timestamp>,
    buffering: bool,
}

impl SimpleSyncState {
    /// Records a written pts. Returns the anchor once the buffer is released.
    fn register_pts(&mut self, pts: Timestamp) -> Option<TimestampAnchor> {
        if !self.buffering {
            return Some(self.anchor());
        };
        let min_pts = self.min_pts.map_or(pts, |min| Timestamp::min(min, pts));
        let max_pts = self.max_pts.map_or(pts, |max| Timestamp::max(max, pts));
        self.min_pts = Some(min_pts);
        self.max_pts = Some(max_pts);

        if max_pts >= min_pts + self.desired_buffer {
            self.buffering = false;
        }

        match self.buffering {
            false => Some(self.anchor()),
            true => None,
        }
    }

    fn anchor(&self) -> TimestampAnchor {
        TimestampAnchor {
            input_pts: self.min_pts.unwrap_or(Timestamp::ZERO),
            output_pts: Timestamp::ZERO,
        }
    }
}

impl SimpleSync {
    pub fn new(desired_buffer: Duration, stats: InputSyncStatsSender) -> Self {
        Self {
            state: Arc::new(Mutex::new(SimpleSyncState {
                desired_buffer,
                min_pts: None,
                max_pts: None,
                buffering: true,
            })),
            stats,
        }
    }

    pub fn add_track<T: InputSyncItem>(
        &self,
        kind: TrackKind,
        sink: BoxedTrackSink<T>,
    ) -> SimpleSyncTrack<T> {
        self.stats.send(
            kind,
            InputSyncTrackStatsEvent::TrackAdded(InputSyncMode::Simple),
        );
        SimpleSyncTrack {
            state: self.state.clone(),
            kind,
            stats: self.stats.clone(),
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
    kind: TrackKind,
    stats: InputSyncStatsSender,
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
        self.stats.send(
            self.kind,
            InputSyncTrackStatsEvent::BytesReceived(chunk.size()),
        );
        let anchor = match self.anchor {
            Some(anchor) => anchor,
            None => match self.state.lock().unwrap().register_pts(chunk.pts()) {
                Some(anchor) => {
                    self.anchor = Some(anchor);
                    self.stats.send(
                        self.kind,
                        InputSyncTrackStatsEvent::Simple(SimpleSyncStatsEvent::StateChanged(
                            SimpleSyncTrackState::Running,
                        )),
                    );
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
        self.stats
            .send(self.kind, InputSyncTrackStatsEvent::TrackRemoved);
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
