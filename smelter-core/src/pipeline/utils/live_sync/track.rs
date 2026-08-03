use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::warn;

use super::{
    LiveSyncOptions,
    buffer::LiveSyncBuffer,
    edge_estimator::LiveEdgeEstimator,
    state::{EdgeSource, SharedState, TimestampAnchor, TrackFlushState},
};
use crate::{
    pipeline::utils::input_sync::InputSyncItem, utils::live_sync::state::resolve_should_start,
};

/// How often the post-start buffer check runs.
const CORRECTION_INTERVAL: Duration = Duration::from_millis(250);
/// pts jump (in either direction) treated as a discontinuity of the input
/// timeline; the old edge estimate does not describe the new timeline.
const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; only feeding the shared estimator and pre-start
/// checks take a short-lived lock on the input's shared state.
pub(crate) struct LiveSyncTrack<B: LiveSyncBuffer> {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    shared: Arc<Mutex<SharedState>>,
    /// Estimator observing only this track's chunks.
    estimator: LiveEdgeEstimator,
    buffer: B,
    /// Buffers rotated out by a reset, paired with the anchor they were
    /// mapped with; drained before `buffer`, oldest first. New data after the
    /// reset collects in a clean `buffer` while the edge is re-estimated.
    flush_queue: VecDeque<(B, TimestampAnchor)>,
    flush_state: TrackFlushState,
    state: TrackState,
    last_correction: Instant,
}

impl<B: LiveSyncBuffer> LiveSyncTrack<B> {
    pub(super) fn new(
        options: LiveSyncOptions,
        sync_point: Instant,
        shared: Arc<Mutex<SharedState>>,
        flush_state: TrackFlushState,
    ) -> Self {
        Self {
            estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
            options,
            sync_point,
            shared,
            buffer: B::default(),
            flush_queue: VecDeque::new(),
            flush_state,
            state: TrackState::WaitingForStart,
            last_correction: Instant::now(),
        }
    }

    pub fn write_chunk(&mut self, item: B::Item) {
        let now = Instant::now();
        self.check_discontinuity(item.pts());
        {
            // both estimators observe for the whole lifetime of the input
            self.estimator.observe(now, item.pts());
            let mut shared = self.shared.lock().unwrap();
            shared.shared_estimator.observe(now, item.pts());
        }

        self.buffer.write(item);
        self.maybe_start();
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// reference timeline; `None` while the live edge is still being detected
    /// or when there is nothing buffered. Chunks buffered before a reset
    /// drain first, mapped with their pre-reset anchor.
    pub fn try_read_chunk(&mut self) -> Option<B::Item> {
        self.maybe_start();
        if let Some(item) = self.read_flushed_chunk() {
            return Some(item);
        }
        let anchor = self.state.anchor()?;
        let mut item = self.buffer.try_read()?;
        item.map_timestamps(|pts| anchor.to_output_pts(pts));
        Some(item)
    }

    /// Output pts of the next readable chunk; `None` while the live edge is
    /// still being detected or when nothing is buffered. Enables interleaved
    /// reads across tracks.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        self.maybe_start();
        self.maybe_correct();
        if let Some(pts) = self.peek_flushed_pts() {
            return Some(pts);
        }
        let anchor = self.state.anchor()?;
        self.buffer
            .peek()
            .map(|item| anchor.to_output_pts(item.pts()))
    }

    /// Next chunk from the flushed buffers, mapped with the anchor it was
    /// buffered under. Gaps in flushed buffers can never be filled, so they
    /// are drained with forced reads; exhausted buffers are dropped.
    fn read_flushed_chunk(&mut self) -> Option<B::Item> {
        loop {
            let (buffer, anchor) = self.flush_queue.front_mut()?;
            match buffer.read() {
                Some(mut item) => {
                    let anchor = *anchor;
                    item.map_timestamps(|pts| anchor.to_output_pts(pts));
                    return Some(item);
                }
                None => {
                    self.flush_queue.pop_front();
                }
            }
        }
    }

    fn peek_flushed_pts(&mut self) -> Option<Duration> {
        loop {
            let (buffer, anchor) = self.flush_queue.front()?;
            match buffer.peek() {
                Some(item) => return Some(anchor.to_output_pts(item.pts())),
                None => {
                    self.flush_queue.pop_front();
                }
            }
        }
    }

    /// Runs the start decision; called without new chunks too, so time-based
    /// conditions can trigger the start when delivery pauses.
    fn maybe_start(&mut self) {
        if !matches!(self.state, TrackState::WaitingForStart) {
            return;
        }
        let now = Instant::now();
        let shared = self.shared.lock().unwrap();

        let estimator = resolve_should_start(
            now,
            &self.options,
            &self.estimator,
            &shared.shared_estimator,
        );
        let Some(estimator) = estimator else {
            return;
        };
        let estimation = match estimator {
            EdgeSource::Shared => shared.shared_estimator.estimate(now),
            EdgeSource::Track => self.estimator.estimate(now),
        };
        let Some(estimation) = estimation else {
            return;
        };

        let stable = estimation.upper_bound.stable_for > self.options.stabilization_period;
        let waited_too_long = estimation.delivery.observed_for > self.options.max_wait;

        if !stable && !waited_too_long {
            return;
        }

        let now_pts = now.saturating_duration_since(self.sync_point);
        let anchor = TimestampAnchor {
            input_pts: estimation.upper_bound.pts,
            output_pts: now_pts + self.options.desired_buffer,
        };

        self.state = TrackState::Started {
            anchor,
            edge_source: estimator,
        };
    }

    //    /// Post-start check of how delivery behaves relative to the playback
    //    /// position: revokes a start based on a false knee, nudges the anchor
    //    /// towards the desired buffer, or resets back to the startup logic when
    //    /// the deviation is beyond correction.
    //    fn maybe_correct(&mut self) {
    //        let now = Instant::now();
    //        if now.saturating_duration_since(self.last_correction) < CORRECTION_INTERVAL {
    //            return;
    //        }
    //        self.last_correction = now;
    //        if self.maybe_reanchor(now) {
    //            return;
    //        }
    //        let Some(anchor) = self.state.anchor() else {
    //            return;
    //        };
    //        let correction = decide_correction(
    //            &self.options,
    //            self.sync_point,
    //            now,
    //            &self.estimator,
    //            &anchor,
    //        );
    //        match correction {
    //            AnchorCorrection::None => (),
    //            AnchorCorrection::Earlier(delta) => {
    //                debug!(?delta, "Live sync buffer too large, presenting earlier");
    //                if let Some(anchor) = self.state.anchor_mut() {
    //                    anchor.shift_earlier(delta);
    //                }
    //            }
    //            AnchorCorrection::Later(delta) => {
    //                debug!(?delta, "Live sync buffer too small, presenting later");
    //                if let Some(anchor) = self.state.anchor_mut() {
    //                    anchor.shift_later(delta);
    //                }
    //            }
    //            AnchorCorrection::Reset => self.reset("buffer diverged beyond correction"),
    //        }
    //    }
    //
    //    /// The chosen edge improving after the start means the start was based on
    //    /// a false knee (e.g. a mid-flush network stall mistaken for the live
    //    /// edge). Revoke it with a single forward jump; the slew would chase an
    //    /// error this size for minutes. Returns `true` when the mapping changed.
    //    fn maybe_reanchor(&mut self, now: Instant) -> bool {
    //        let (upper_edge, edge_offset) = match &self.state {
    //            TrackState::WaitingForStart => return false,
    //            TrackState::StartedWithTrackEstimator { edge_offset, .. } => {
    //                let bounds = self.estimator.edge_bounds(now);
    //                (bounds.map(|bounds| bounds.upper), *edge_offset)
    //            }
    //            TrackState::StartedWithSharedEstimator { edge_offset, .. } => {
    //                let bounds = self
    //                    .shared
    //                    .lock()
    //                    .unwrap()
    //                    .shared_estimator
    //                    .edge_bounds(now);
    //                (bounds.map(|bounds| bounds.upper), *edge_offset)
    //            }
    //        };
    //        let Some(upper_edge) = upper_edge else {
    //            return false;
    //        };
    //        let elapsed = now.saturating_duration_since(self.sync_point);
    //        let current_offset = elapsed.saturating_sub(upper_edge);
    //        let improvement = edge_offset.saturating_sub(current_offset);
    //        if improvement <= self.options.stabilization_tolerance {
    //            return false;
    //        }
    //        info!(?improvement, "Live edge improved after start, re-anchoring");
    //        match &mut self.state {
    //            TrackState::WaitingForStart => (),
    //            TrackState::StartedWithSharedEstimator {
    //                anchor,
    //                edge_offset,
    //            }
    //            | TrackState::StartedWithTrackEstimator {
    //                anchor,
    //                edge_offset,
    //            } => {
    //                anchor.shift_earlier(improvement);
    //                *edge_offset = current_offset;
    //            }
    //        }
    //        true
    //    }

    /// A pts jump beyond [`DISCONTINUITY_THRESHOLD`] means the old estimate
    /// does not describe the input timeline anymore; estimation starts over.
    fn check_discontinuity(&mut self, pts: Duration) {
        let Some(max_pts) = self.estimator.max_pts() else {
            return;
        };
        let forward_jump = pts > max_pts + DISCONTINUITY_THRESHOLD;
        let backward_jump = pts + DISCONTINUITY_THRESHOLD < max_pts;
        if forward_jump || backward_jump {
            self.reset("pts discontinuity");
        }
    }

    fn reset(&mut self, reason: &str) {
        warn!(reason, "Live sync track reset, re-estimating the live edge");
        if let Some(anchor) = self.state.anchor() {
            self.flush_queue
                .push_back((std::mem::take(&mut self.buffer), anchor));
        }
        self.estimator =
            LiveEdgeEstimator::new(self.sync_point, self.options.stabilization_tolerance);
        self.state = TrackState::WaitingForStart;
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
    Started {
        anchor: TimestampAnchor,
        edge_source: EdgeSource,
    },
}

impl TrackState {
    fn anchor(&self) -> Option<TimestampAnchor> {
        match self {
            TrackState::WaitingForStart => None,
            TrackState::Started { anchor, .. } => Some(*anchor),
        }
    }

    fn anchor_mut(&mut self) -> Option<&mut TimestampAnchor> {
        match self {
            TrackState::WaitingForStart => None,
            TrackState::Started { anchor, .. } => Some(anchor),
        }
    }
}
