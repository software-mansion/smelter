use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::{debug, info, warn};

use super::{
    LiveSyncOptions,
    edge_estimator::LiveEdgeEstimator,
    state::{
        AnchorCorrection, EdgeSource, SharedState, TimestampAnchor, decide_correction, decide_start,
    },
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
pub(crate) struct LiveSyncTrack<T: InputSyncItem> {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    shared: Arc<Mutex<SharedState>>,
    /// Estimator observing only this track's chunks.
    estimator: LiveEdgeEstimator,
    buffer: VecDeque<T>,
    state: TrackState,
    last_correction: Instant,
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
            last_correction: Instant::now(),
        }
    }

    pub fn write_chunk(&mut self, item: T) {
        let now = Instant::now();
        self.check_discontinuity(item.pts());
        {
            // both estimators observe for the whole lifetime of the input
            self.estimator.observe(now, item.pts());
            let mut shared = self.shared.lock().unwrap();
            shared.shared_estimator.observe(now, item.pts());
        }

        self.buffer.push_back(item);
        self.maybe_start();
        self.maybe_correct();
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// reference timeline; `None` while the live edge is still being detected
    /// or when there is nothing buffered.
    pub fn try_read_chunk(&mut self) -> Option<T> {
        self.maybe_start();
        self.maybe_correct();
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
        self.maybe_correct();
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


        estimation.delivery.


        // start when:
        // - both estimators are stable
        // - I have enough data to fill MIN buffer
        //
        // at the start:
        // _ current buffer needs to be "estimated" from estimator, otherwise tracks might get out
        // of sync.
        // - current calculate anchor for current edge (elapsed + buffer)
        //   - if current buffer state > (max+min)/2 then move anchor by that value
        //   - if current buffer state < (max+min)/2 but > min, use that value, do not cut of
        //   anything
        //   - if current buffer state < min, do nothing wait for more data

        let shared_bounds = shared.shared_estimator.edge_bounds(now);
        let flushed = shared.flushed;
        drop(shared);

        let Some(decision) = decide_start(
            &self.options,
            self.sync_point,
            now,
            &self.estimator,
            shared_bounds,
            flushed,
        ) else {
            return;
        };

        self.state = match decision.edge {
            EdgeSource::Shared => TrackState::StartedWithSharedEstimator {
                anchor: decision.anchor,
                edge_offset: decision.edge_offset,
            },
            EdgeSource::Track => TrackState::StartedWithTrackEstimator {
                anchor: decision.anchor,
                edge_offset: decision.edge_offset,
            },
        };
    }

    /// Post-start check of how delivery behaves relative to the playback
    /// position: revokes a start based on a false knee, nudges the anchor
    /// towards the desired buffer, or resets back to the startup logic when
    /// the deviation is beyond correction.
    fn maybe_correct(&mut self) {
        let now = Instant::now();
        if now.saturating_duration_since(self.last_correction) < CORRECTION_INTERVAL {
            return;
        }
        self.last_correction = now;
        if self.maybe_reanchor(now) {
            return;
        }
        let Some(anchor) = self.state.anchor() else {
            return;
        };
        let correction = decide_correction(
            &self.options,
            self.sync_point,
            now,
            &self.estimator,
            &anchor,
        );
        match correction {
            AnchorCorrection::None => (),
            AnchorCorrection::Earlier(delta) => {
                debug!(?delta, "Live sync buffer too large, presenting earlier");
                if let Some(anchor) = self.state.anchor_mut() {
                    anchor.shift_earlier(delta);
                }
            }
            AnchorCorrection::Later(delta) => {
                debug!(?delta, "Live sync buffer too small, presenting later");
                if let Some(anchor) = self.state.anchor_mut() {
                    anchor.shift_later(delta);
                }
            }
            AnchorCorrection::Reset => self.reset("buffer diverged beyond correction"),
        }
    }

    /// The chosen edge improving after the start means the start was based on
    /// a false knee (e.g. a mid-flush network stall mistaken for the live
    /// edge). Revoke it with a single forward jump; the slew would chase an
    /// error this size for minutes. Returns `true` when the mapping changed.
    fn maybe_reanchor(&mut self, now: Instant) -> bool {
        let (upper_edge, edge_offset) = match &self.state {
            TrackState::WaitingForStart => return false,
            TrackState::StartedWithTrackEstimator { edge_offset, .. } => {
                let bounds = self.estimator.edge_bounds(now);
                (bounds.map(|bounds| bounds.upper), *edge_offset)
            }
            TrackState::StartedWithSharedEstimator { edge_offset, .. } => {
                let bounds = self
                    .shared
                    .lock()
                    .unwrap()
                    .shared_estimator
                    .edge_bounds(now);
                (bounds.map(|bounds| bounds.upper), *edge_offset)
            }
        };
        let Some(upper_edge) = upper_edge else {
            return false;
        };
        let elapsed = now.saturating_duration_since(self.sync_point);
        let current_offset = elapsed.saturating_sub(upper_edge);
        let improvement = edge_offset.saturating_sub(current_offset);
        if improvement <= self.options.stabilization_tolerance {
            return false;
        }
        info!(?improvement, "Live edge improved after start, re-anchoring");
        match &mut self.state {
            TrackState::WaitingForStart => (),
            TrackState::StartedWithSharedEstimator {
                anchor,
                edge_offset,
            }
            | TrackState::StartedWithTrackEstimator {
                anchor,
                edge_offset,
            } => {
                anchor.shift_earlier(improvement);
                *edge_offset = current_offset;
            }
        }
        true
    }

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
    StartedWithSharedEstimator {
        anchor: TimestampAnchor,
        /// Delivery offset of the shared edge at the start; reference for
        /// detecting post-start edge improvements.
        edge_offset: Duration,
    },
    /// Also the state after a flush; the flush anchor is derived from the
    /// track's own data.
    StartedWithTrackEstimator {
        anchor: TimestampAnchor,
        /// Delivery offset of the track's own edge at the start; reference
        /// for detecting post-start edge improvements.
        edge_offset: Duration,
    },
}

impl TrackState {
    fn anchor(&self) -> Option<TimestampAnchor> {
        match self {
            TrackState::WaitingForStart => None,
            TrackState::StartedWithSharedEstimator { anchor, .. }
            | TrackState::StartedWithTrackEstimator { anchor, .. } => Some(*anchor),
        }
    }

    fn anchor_mut(&mut self) -> Option<&mut TimestampAnchor> {
        match self {
            TrackState::WaitingForStart => None,
            TrackState::StartedWithSharedEstimator { anchor, .. }
            | TrackState::StartedWithTrackEstimator { anchor, .. } => Some(anchor),
        }
    }
}
