use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::{
    LiveSyncOptions,
    edge_estimator::LiveEdgeEstimator,
    state::{EdgeSource, SharedState, TimestampAnchor, decide_start},
};
use crate::pipeline::utils::input_sync::InputSyncItem;

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; only feeding the shared estimator and pre-start
/// checks take a short-lived lock on the input's shared state.
pub(crate) struct LiveSyncTrack<T: InputSyncItem> {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    shared: Arc<Mutex<SharedState>>,
    /// Estimator observing only this track's chunks.
    estimator: LiveEdgeEstimator,
    buffer: VecDeque<T>,
    state: TrackState,
}

impl<T: InputSyncItem> LiveSyncTrack<T> {
    pub(super) fn new(
        options: LiveSyncOptions,
        sync_point: Instant,
        shared: Arc<Mutex<SharedState>>,
    ) -> Self {
        Self {
            estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
            options,
            sync_point,
            shared,
            buffer: VecDeque::new(),
            state: TrackState::WaitingForStart,
        }
    }

    pub fn write_chunk(&mut self, item: T) {
        {
            // both estimators observe for the whole lifetime of the input
            let now = Instant::now();
            self.estimator.observe(now, item.pts());
            let mut shared = self.shared.lock().unwrap();
            shared.shared_estimator.observe(now, item.pts());
        }

        self.buffer.push_back(item);
        self.maybe_start();
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// reference timeline; `None` while the live edge is still being detected
    /// or when there is nothing buffered.
    pub fn try_read_chunk(&mut self) -> Option<T> {
        self.maybe_start();
        let anchor = self.state.anchor()?;
        let mut item = self.buffer.pop_front()?;
        item.map_timestamps(|pts| anchor.to_output_pts(pts));
        Some(item)
    }

    /// Output pts of the next readable chunk; `None` while the live edge is
    /// still being detected or when nothing is buffered. Enables interleaved
    /// reads across tracks.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        self.maybe_start();
        let anchor = self.state.anchor()?;
        self.buffer
            .front()
            .map(|item| anchor.to_output_pts(item.pts()))
    }

    /// Runs the start decision; called without new chunks too, so time-based
    /// conditions can trigger the start when delivery pauses.
    fn maybe_start(&mut self) {
        if !matches!(self.state, TrackState::WaitingForStart) {
            return;
        }
        let now = Instant::now();

        let shared = self.shared.lock().unwrap();
        let shared_estimate = shared.shared_estimator.estimate(now);
        let flushed = shared.flushed;
        drop(shared);

        let track_estimate = self.estimator.estimate(now);

        let Some(decision) = decide_start(
            &self.options,
            self.sync_point,
            now,
            track_estimate,
            shared_estimate,
            flushed,
        ) else {
            return;
        };

        self.state = match decision.edge {
            EdgeSource::Shared => TrackState::StartedWithSharedEstimator {
                anchor: decision.anchor,
            },
            EdgeSource::Track => TrackState::StartedWithTrackEstimator {
                anchor: decision.anchor,
            },
        };
    }
}

enum TrackState {
    /// Written chunks are buffered and never returned. On each write
    /// we are checking if both edge estimators are ready.
    ///
    /// If shared and track estimator diverge by more than
    /// `shared_edge_tolerance` switch to `StartedWithTrackEstimator`,
    /// otherwise switch to `StartedWithSharedEstimator`.
    WaitingForStart,
    StartedWithSharedEstimator { anchor: TimestampAnchor },
    /// Also the state after a flush; the flush anchor is derived from the
    /// track's own data.
    StartedWithTrackEstimator { anchor: TimestampAnchor },
}

impl TrackState {
    fn anchor(&self) -> Option<TimestampAnchor> {
        match self {
            TrackState::WaitingForStart => None,
            TrackState::StartedWithSharedEstimator { anchor }
            | TrackState::StartedWithTrackEstimator { anchor } => Some(*anchor),
        }
    }
}
