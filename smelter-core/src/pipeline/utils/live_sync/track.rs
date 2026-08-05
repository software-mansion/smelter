use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::{
    LiveSyncOptions,
    buffer::LiveSyncBuffer,
    edge_estimator::LiveEdgeEstimator,
    flush::{FlushQueue, TrackFlushState},
    state::{EdgeSource, SharedState, TimestampAnchor},
};
use crate::{
    pipeline::utils::input_sync::InputSyncItem, utils::live_sync::state::resolve_should_start,
};

/// pts jump (in either direction) treated as a discontinuity of the input
/// timeline; the old edge estimate does not describe the new timeline.
const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

/// Lead over the playback position content needs to still reach the queue.
const MIN_QUEUE_HEADROOM: Duration = Duration::from_millis(80);

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; only feeding the shared estimator and pre-start
/// checks take a short-lived lock on the input's shared state.
pub(crate) struct LiveSyncTrack<B: LiveSyncBuffer> {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,

    shared: Arc<Mutex<SharedState>>,

    state: TrackState,

    /// Estimator observing only this track's chunks.
    estimator: LiveEdgeEstimator,

    // Current buffer
    buffer: B,

    // When we detect discontinuity current state of the buffer still needs
    // to be returned, but we already need to "observe" the packet that caused
    // discontinuity
    flush_queue: FlushQueue<B>,
    flush_state: TrackFlushState,
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
            flush_queue: FlushQueue::default(),
            flush_state,
            state: TrackState::WaitingForStart,
        }
    }

    pub fn write_chunk(&mut self, item: B::Item) {
        if self.flush_state.should_flush() {
            self.reset();
        }

        let now = Instant::now();
        self.check_discontinuity(now, item.pts());

        {
            // both estimators observe for the whole lifetime of the input
            self.estimator.observe(now, item.pts());
            let mut shared = self.shared.lock().unwrap();
            shared.shared_estimator.observe(now, item.pts());
            self.buffer.write(item);
        }

        self.maybe_start();
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// reference timeline; `None` while the live edge is still being detected
    /// or when there is nothing buffered. Chunks buffered before a reset
    /// drain first, mapped with their pre-reset anchor.
    pub fn try_read_chunk(&mut self) -> Option<B::Item> {
        if self.flush_state.should_flush() {
            self.reset();
        }
        self.maybe_start();

        if let Some(item) = self.flush_queue.read() {
            return Some(item);
        }

        // check current buffer
        let anchor = self.state.anchor()?;
        let mut chunk = self.buffer.try_read()?;
        chunk.map_timestamps(|pts| anchor.to_output_pts(pts));
        Some(chunk)
    }

    /// Output pts of the next readable chunk; `None` while the live edge is
    /// still being detected or when nothing is buffered. Enables interleaved
    /// reads across tracks.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        if self.flush_state.should_flush() {
            self.reset();
        }

        self.maybe_start();

        if let Some(pts) = self.flush_queue.peek_pts() {
            return Some(pts);
        }

        // check current buffer
        let anchor = self.state.anchor()?; // TODO: it should return even if no anchor (the same behavior as flush)
        let chunk = self.buffer.peek()?;
        Some(anchor.to_output_pts(chunk.pts()))
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

    fn best_effort_anchor(&self, now: Instant) -> Option<TimestampAnchor> {
        let now_pts = now.saturating_duration_since(self.sync_point);
        // Continue where the flushed content ends, unless it ends too close
        // to the playback position to still reach the queue.
        let output_pts = match self.flush_queue.end_pts() {
            Some(end_pts) if end_pts > now_pts + MIN_QUEUE_HEADROOM => end_pts,
            _ => now_pts + self.options.desired_buffer,
        };
        Some(TimestampAnchor {
            input_pts: self.buffer.peek()?.pts(),
            output_pts,
        })
    }

    fn check_discontinuity(&mut self, now: Instant, pts: Duration) {
        let Some(estimation) = self.estimator.estimate(now) else {
            return;
        };
        let delivery = estimation.delivery;
        // pts expected if the stream kept producing in real time since the
        // newest received chunk
        let expected_pts = delivery.last_pts + delivery.since_last_arrival;
        let forward_jump = pts > expected_pts + DISCONTINUITY_THRESHOLD;
        let backward_jump = pts + DISCONTINUITY_THRESHOLD < delivery.last_pts;
        if forward_jump || backward_jump {
            self.flush_state.flush();
            self.reset();
        }
    }

    fn reset(&mut self) {
        let now = Instant::now();
        // the estimator only observed this timeline, so its newest pts covers
        // everything the flushed buffer can release
        let last_pts = self.estimator.estimate(now).map(|e| e.delivery.last_pts);
        let anchor = self.state.anchor().or_else(|| self.best_effort_anchor(now));

        self.estimator =
            LiveEdgeEstimator::new(self.sync_point, self.options.stabilization_tolerance);
        self.state = TrackState::WaitingForStart;

        if let Some(anchor) = anchor {
            let buffer = std::mem::take(&mut self.buffer);
            self.flush_queue.push(buffer, anchor, last_pts);
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
}
