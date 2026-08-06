use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::debug;

use super::{
    LiveSyncOptions,
    anchor::{SlewingAnchor, TimestampAnchor},
    buffer::LiveSyncBuffer,
    edge_estimator::{EdgeEstimate, LiveEdgeEstimator},
    flush::{FlushQueue, TrackFlushState},
    state::{EdgeSource, SharedState, resolve_should_start},
};
use crate::pipeline::utils::input_sync::InputSyncItem;

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
        self.maybe_correct();

        if let Some(item) = self.flush_queue.read() {
            return Some(item);
        }

        // current buffer is held back until the live edge is found
        let TrackState::Started { anchor, .. } = &mut self.state else {
            return None;
        };
        let mut chunk = self.buffer.try_read()?;
        anchor.map_chunk(&mut chunk);
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
        self.maybe_correct();

        if let Some(pts) = self.flush_queue.peek_pts() {
            return Some(pts);
        }

        // check current buffer; a correction only advances when the chunk is
        // read, so the pts reported here can be one step off
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
        // target from the track's own delivery; the shared estimator observes
        // the arrivals of all tracks interleaved, so its gap underestimates
        // how long this track goes without a refill
        let Some(own) = self.estimator.estimate(now) else {
            return;
        };

        let now_pts = now.saturating_duration_since(self.sync_point);
        let anchor = TimestampAnchor {
            input_pts: estimation.upper_bound.pts,
            output_pts: now_pts + self.target_buffer(&own),
        };

        self.state = TrackState::Started {
            anchor: SlewingAnchor::new(anchor),
            edge_source: estimator,
        };
    }

    /// Buffer the mapping aims for: `desired_buffer`, raised for batched
    /// delivery so that it survives the gap between two batches.
    fn target_buffer(&self, own: &EdgeEstimate) -> Duration {
        Duration::max(
            self.options.desired_buffer,
            own.delivery.max_arrival_gap * 3 / 2,
        )
    }

    /// Checks how much content is buffered ahead of the playback position and
    /// starts a correction when it drifted away from the target.
    ///
    /// The correction keeps the anchor it was started with until the mapping
    /// reaches it, so tracks aligned to the same edge converge to the same
    /// mapping instead of each following the jitter of its own estimate.
    fn maybe_correct(&mut self) {
        if self.state.is_correcting() {
            return;
        }
        let Some(anchor) = self.state.anchor() else {
            return;
        };
        let now = Instant::now();
        // how much is buffered is measured against this track's own delivery;
        // the shared estimator reports whichever track is freshest, so a
        // starving track would look healthy because a sibling keeps delivering
        let Some(own) = self.estimator.estimate(now) else {
            return;
        };
        let now_pts = now.saturating_duration_since(self.sync_point);
        let target = self.target_buffer(&own);
        let buffered = anchor
            .to_output_pts(own.delivery.last_pts)
            .saturating_sub(now_pts);

        // the allowed range is expressed around `desired_buffer`, so a target
        // raised for batched delivery moves its upper limit up as well
        let max_buffer = self.options.max_buffer + (target - self.options.desired_buffer);
        if buffered >= self.options.min_buffer && buffered <= max_buffer {
            return;
        }
        let Some(edge) = self.edge_estimate(now) else {
            return;
        };
        debug!(
            ?buffered,
            ?target,
            "Live sync buffer off target, correcting"
        );
        self.state.correct_to(TimestampAnchor {
            input_pts: edge.upper_bound.pts,
            output_pts: now_pts + target,
        });
    }

    /// Estimate of the edge this track aligned to when it started.
    fn edge_estimate(&self, now: Instant) -> Option<EdgeEstimate> {
        let TrackState::Started { edge_source, .. } = &self.state else {
            return None;
        };
        match edge_source {
            EdgeSource::Track => self.estimator.estimate(now),
            EdgeSource::Shared => self.shared.lock().unwrap().shared_estimator.estimate(now),
        }
    }

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
    /// Written chunks are buffered and never returned. On each write we are
    /// checking if both edge estimators are ready.
    WaitingForStart,
    Started {
        anchor: SlewingAnchor,
        /// Edge the anchor was aligned to; corrections keep using it.
        edge_source: EdgeSource,
    },
}

impl TrackState {
    fn anchor(&self) -> Option<TimestampAnchor> {
        match self {
            TrackState::WaitingForStart => None,
            TrackState::Started { anchor, .. } => Some(anchor.current()),
        }
    }

    fn is_correcting(&self) -> bool {
        match self {
            TrackState::WaitingForStart => false,
            TrackState::Started { anchor, .. } => anchor.is_correcting(),
        }
    }

    fn correct_to(&mut self, destination: TimestampAnchor) {
        if let TrackState::Started { anchor, .. } = self {
            anchor.correct_to(destination);
        }
    }
}
