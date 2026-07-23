use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::debug;

use super::state::{SharedState, StartDecision, TrackMeta};
use crate::pipeline::utils::input_sync::InputSyncItem;

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; only pre-start calls synchronize with other tracks.
pub(crate) struct LiveSyncTrack<T: InputSyncItem> {
    shared: Arc<SharedState>,
    track_index: usize,
    buffer: VecDeque<T>,
    /// Cached shared decision; set once, never changes afterwards.
    start: Option<StartDecision>,
}

impl<T: InputSyncItem> LiveSyncTrack<T> {
    pub(super) fn new(shared: Arc<SharedState>) -> Self {
        let mut detection = shared.detection.lock().unwrap();
        detection.tracks.push(TrackMeta::default());
        let track_index = detection.tracks.len() - 1;
        let start = detection.start;
        drop(detection);
        Self {
            shared,
            track_index,
            buffer: VecDeque::new(),
            start,
        }
    }

    pub fn write_chunk(&mut self, item: T) {
        if self.start.is_some() {
            self.buffer.push_back(item);
            return;
        }
        let now = Instant::now();
        let decision = {
            let mut detection = self.shared.detection.lock().unwrap();
            if detection.start.is_none() {
                detection.observe(self.track_index, now, item.pts(), item.is_keyframe());
                detection.maybe_start(&self.shared, now);
            }
            detection.start
        };
        self.buffer.push_back(item);
        self.apply_decision(decision);
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// reference timeline; `None` while the live edge is still being detected
    /// or when there is nothing buffered.
    pub fn try_read_chunk(&mut self) -> Option<T> {
        if self.start.is_none() {
            let decision = {
                let mut detection = self.shared.detection.lock().unwrap();
                if detection.start.is_none() {
                    detection.maybe_start(&self.shared, Instant::now());
                }
                detection.start
            };
            self.apply_decision(decision);
        }
        let start = self.start?;
        let mut item = self.buffer.pop_front()?;
        item.map_timestamps(|pts| start.anchor.to_output_pts(pts));
        Some(item)
    }

    // Current estimation of the next pts, it is possible that value will change
    // when packet is popped. This function is intended to allow interleaved read, so
    // upstream channel does not get stuck by draining one track first
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        None
    }

    pub fn has_buffered_chunks(&self) -> bool {
        !self.buffer.is_empty()
    }

    fn apply_decision(&mut self, decision: Option<StartDecision>) {
        let Some(decision) = decision else {
            return;
        };
        if let Some(cut_pts) = decision.cut_pts {
            self.trim(cut_pts);
        }
        self.start = Some(decision);
    }

    /// Drop buffered items so playback starts near `cut_pts`.
    fn trim(&mut self, cut_pts: Duration) {
        let buffered_before = self.buffer.len();
        if self.buffer.iter().all(|item| item.is_keyframe()) {
            self.buffer.retain(|item| item.pts() >= cut_pts);
        } else {
            // cut in write order at the last start point before `cut_pts`
            // (fall back to the first start point, items before it are not
            // decodable anyway)
            let mut cut_index = None;
            for (index, item) in self.buffer.iter().enumerate() {
                if item.is_keyframe() && (item.pts() <= cut_pts || cut_index.is_none()) {
                    cut_index = Some(index);
                }
            }
            self.buffer.drain(0..cut_index.unwrap_or(0));
        }
        debug!(
            dropped_chunks = buffered_before - self.buffer.len(),
            buffered_chunks = self.buffer.len(),
            "Trimmed track buffer on live sync start"
        );
    }
}
