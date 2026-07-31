//! Track synchronization for the RTMP input, supporting both live and
//! non-live streams.
//!
//! Each variant defines its own output timeline, so the queue track has to be
//! registered to match the variant used:
//! - [`InputSync::Live`] ([`LiveSync`]) maps timestamps onto the timeline of
//!   the queue sync point; register with `QueueTrackOffset::Pts(Duration::ZERO)`.
//! - [`InputSync::Simple`] ([`SimpleSync`]) normalizes timestamps to start at
//!   zero; register with `QueueTrackOffset::None` so the queue fixes the
//!   placement on the first received packet.
//!
//! Chunks read from a track already have their timestamps mapped onto the
//! output timeline.
//!
//! # Live synchronization
//!
//! Live protocols rarely deliver data at a real time rate right after
//! connecting; RTMP clients can flush a few seconds of pre-buffered chunks.
//! If playback timing is decided when the connection is established, that
//! initial backlog ends up stretched, squashed or dropped by the consumer.
//!
//! [`LiveSync`] runs a single [`LiveEdgeEstimator`] observing the chunks of
//! all tracks:
//! - For every chunk it samples `offset = arrival_time - pts`; the recent
//!   extremes of the offset bound the live edge (the minimum yields the upper
//!   bound, extrapolated from the freshest delivery seen). The window makes
//!   the bounds follow changes of the network latency instead of locking to
//!   lifetime extremes.
//! - When the upper bound stops improving for `stabilization_period`,
//!   delivery reached a real time rate (dropped to or below it) and the
//!   estimate is considered ready. This works for batched delivery too: the
//!   silence after a batch is itself the signal, so readiness does not depend
//!   on the batch size.
//!
//! The sync starts once the estimate is stable (`max_wait` bounds the wait as
//! a safety valve): the newest chunk buffered by the track that triggered the
//! start is anchored `desired_buffer` behind the playback position, so
//! playback starts with exactly the desired buffer; any older backlog maps
//! before the start point and plays late or is dropped by the consumer.
//!
//! After the start the buffer (newest delivered content relative to the
//! playback position) is checked against the `min_buffer..max_buffer` band:
//! - While the buffer is out of bounds, the shared correction target is
//!   updated: the mapping that would put the buffer back at `desired_buffer`,
//!   with a rate scaling with how far past the bound the buffer is (minimal
//!   just past it, the full rate at twice `max_buffer`, or at an empty buffer
//!   on the other side).
//! - Each track converges its own mapping towards the target as its content
//!   is read; a step is bounded by the rate times the content progress since
//!   the last step, so content is never stretched or squashed by more than
//!   the rate (at 4%, 100ms of content plays as 96..104ms). Corrections
//!   follow content rather than wall time, so tracks cannot desynchronize
//!   against each other: a track resuming after a stall converges over the
//!   content it reads, tracing the same rate-bounded path the other tracks
//!   took, instead of jumping to their accumulated correction.
//! - A buffer diverged too far past the band would take minutes to slew
//!   back; the sync resets back to the startup logic instead and the live
//!   edge gets re-estimated.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::{debug, warn};

use crate::pipeline::utils::{
    live_edge_estimator::LiveEdgeEstimator, timestamp_anchor::TimestampAnchor,
};
use crate::prelude::*;

/// Synchronization of a single input; create per-track handles with
/// [`InputSync::add_track`].
pub(crate) enum InputSync {
    Live(LiveSync),
    Simple(SimpleSync),
}

impl InputSync {
    pub fn add_track(&self) -> InputSyncTrack {
        match self {
            InputSync::Live(sync) => InputSyncTrack::Live(sync.add_track()),
            InputSync::Simple(sync) => InputSyncTrack::Simple(sync.add_track()),
        }
    }

    /// Give up on any pending detection and release everything that is
    /// buffered (e.g. when the stream ended).
    pub fn flush(&self) {
        match self {
            InputSync::Live(sync) => sync.flush(),
            // SimpleSync never holds chunks back
            InputSync::Simple(_) => (),
        }
    }
}

pub(crate) enum InputSyncTrack {
    Live(LiveSyncTrack),
    Simple(SimpleSyncTrack),
}

impl InputSyncTrack {
    pub fn write_chunk(&mut self, chunk: EncodedInputChunk) {
        match self {
            InputSyncTrack::Live(track) => track.write_chunk(chunk),
            InputSyncTrack::Simple(track) => track.write_chunk(chunk),
        }
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// output timeline; `None` when no chunk can be produced right now.
    pub fn try_read_chunk(&mut self) -> Option<EncodedInputChunk> {
        match self {
            InputSyncTrack::Live(track) => track.try_read_chunk(),
            InputSyncTrack::Simple(track) => track.try_read_chunk(),
        }
    }

    /// Pts of the next readable chunk; enables interleaved reads across
    /// tracks.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        match self {
            InputSyncTrack::Live(track) => track.peek_next_pts(),
            InputSyncTrack::Simple(track) => track.peek_next_pts(),
        }
    }
}

/// Synchronization for non-live inputs: normalizes timestamps of all tracks
/// to start at zero, based on the first chunk written to any track. Chunks
/// are never held back; a chunk can be read as soon as it is written.
pub(crate) struct SimpleSync {
    first_pts: Arc<Mutex<Option<Duration>>>,
}

impl SimpleSync {
    pub fn new() -> Self {
        Self {
            first_pts: Arc::new(Mutex::new(None)),
        }
    }

    pub fn add_track(&self) -> SimpleSyncTrack {
        SimpleSyncTrack {
            first_pts: self.first_pts.clone(),
            buffer: VecDeque::new(),
        }
    }
}

pub(crate) struct SimpleSyncTrack {
    first_pts: Arc<Mutex<Option<Duration>>>,
    buffer: VecDeque<EncodedInputChunk>,
}

impl SimpleSyncTrack {
    pub fn write_chunk(&mut self, chunk: EncodedInputChunk) {
        self.first_pts.lock().unwrap().get_or_insert(chunk.pts);
        self.buffer.push_back(chunk);
    }

    pub fn try_read_chunk(&mut self) -> Option<EncodedInputChunk> {
        let mut chunk = self.buffer.pop_front()?;
        let first_pts = self.first_pts.lock().unwrap().unwrap_or(Duration::ZERO);
        chunk.pts = chunk.pts.saturating_sub(first_pts);
        chunk.dts = chunk.dts.map(|dts| dts.saturating_sub(first_pts));
        Some(chunk)
    }

    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        self.buffer.front().map(|chunk| chunk.pts)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncOptions {
    /// Correction kicks in when the buffer drops below this.
    pub min_buffer: Duration,
    /// Buffer size the correction slews back towards.
    pub desired_buffer: Duration,
    /// Correction kicks in when the buffer grows beyond this.
    pub max_buffer: Duration,
    /// How long the live edge estimate has to stay stable before starting.
    pub stabilization_period: Duration,
    /// Estimate improvements smaller than this (delivery jitter) do not reset
    /// the stabilization timer.
    pub stabilization_tolerance: Duration,
    /// Start with the current estimate if the live edge was not detected
    /// within this much time from the first chunk.
    pub max_wait: Duration,
}

impl LiveSyncOptions {
    pub fn with_desired_buffer(desired_buffer: Duration) -> Self {
        Self {
            min_buffer: desired_buffer / 3,
            desired_buffer,
            max_buffer: desired_buffer * 2,
            stabilization_period: Duration::from_secs(2),
            stabilization_tolerance: Duration::from_millis(200),
            max_wait: desired_buffer + Duration::from_secs(8),
        }
    }
}

/// How often the post-start buffer check runs.
const CORRECTION_INTERVAL: Duration = Duration::from_millis(250);
/// Hard cap of the correction speed as a fraction of content time: 100ms of
/// content is stretched to at most 104ms or squashed to at least 96ms.
const MAX_CORRECTION_RATE: f64 = 0.04;
/// Buffer deviation past the band that is treated as an anomaly (pts
/// discontinuity, long stall): slewing it back would take minutes, so the
/// sync resets and the live edge gets re-estimated.
const RESET_THRESHOLD: Duration = Duration::from_secs(10);

/// Synchronization of a single live input; create per-track handles with
/// [`LiveSync::add_track`].
pub(crate) struct LiveSync {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    shared: Arc<Mutex<SharedState>>,
}

impl LiveSync {
    pub fn new(options: LiveSyncOptions, sync_point: Instant) -> Self {
        Self {
            shared: Arc::new(Mutex::new(SharedState {
                estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
                flushed: false,
                target: None,
                last_check: Instant::now(),
            })),
            options,
            sync_point,
        }
    }

    /// Registers a new track. Tracks share the live edge detection and the
    /// correction target, but each converges its own mapping.
    pub fn add_track(&self) -> LiveSyncTrack {
        LiveSyncTrack {
            options: self.options,
            sync_point: self.sync_point,
            shared: self.shared.clone(),
            buffer: VecDeque::new(),
            anchor: None,
            stepped_pts: None,
        }
    }

    /// Give up on live edge detection; each track releases everything it
    /// buffered on its next call (e.g. when the stream ended before the live
    /// edge was detected).
    pub fn flush(&self) {
        self.shared.lock().unwrap().flushed = true;
    }
}

/// Cross-track mutable state of an input, kept behind a mutex.
struct SharedState {
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    estimator: LiveEdgeEstimator,
    /// Live edge detection was abandoned (e.g. the stream ended before the
    /// edge was detected); tracks release everything they buffered.
    flushed: bool,
    /// Mapping all tracks converge to; `None` until the sync starts and
    /// after a reset.
    target: Option<CorrectionTarget>,
    /// Last time the post-start buffer check ran.
    last_check: Instant,
}

/// Shared correction state: where every track's mapping should end up and
/// how fast it may move there.
#[derive(Debug, Clone, Copy)]
struct CorrectionTarget {
    anchor: TimestampAnchor,
    /// Fraction of content time a converging track may stretch or squash;
    /// zero outside of corrections.
    rate: f64,
}

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; the estimator feed and the start/correction checks
/// take a short-lived lock on the input's shared state.
pub(crate) struct LiveSyncTrack {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    shared: Arc<Mutex<SharedState>>,
    buffer: VecDeque<EncodedInputChunk>,
    /// This track's mapping; converges towards the shared target.
    anchor: Option<TimestampAnchor>,
    /// Input pts up to which corrections were already applied.
    stepped_pts: Option<Duration>,
}

impl LiveSyncTrack {
    pub fn write_chunk(&mut self, chunk: EncodedInputChunk) {
        let now = Instant::now();
        let now_pts = now.saturating_duration_since(self.sync_point);
        let pts = chunk.pts;
        self.buffer.push_back(chunk);
        let mut shared = self.shared.lock().unwrap();
        shared.estimator.observe(now, pts);
        self.update_shared(&mut shared, now, now_pts);
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// output timeline; `None` while the live edge is still being detected or
    /// when there is nothing buffered.
    pub fn try_read_chunk(&mut self) -> Option<EncodedInputChunk> {
        let target = self.sync_target()?;
        let anchor = self.converge_anchor(target, self.buffer.front()?.pts);
        let mut chunk = self.buffer.pop_front()?;
        chunk.pts = anchor.to_output_pts(chunk.pts);
        chunk.dts = chunk.dts.map(|dts| anchor.to_output_pts(dts));
        Some(chunk)
    }

    /// Output pts of the next readable chunk; `None` while the live edge is
    /// still being detected or when nothing is buffered. Enables interleaved
    /// reads across tracks. Does not converge the mapping; the returned pts
    /// can differ from the read one by up to one convergence step.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        let target = self.sync_target()?;
        let anchor = self.anchor.unwrap_or(target.anchor);
        let pts = self.buffer.front()?.pts;
        Some(anchor.to_output_pts(pts))
    }

    /// Runs the start/correction checks and returns the current shared
    /// target; `None` while the sync has not started. Called on reads too, so
    /// time-based conditions can trigger the start when delivery pauses.
    fn sync_target(&mut self) -> Option<CorrectionTarget> {
        let now = Instant::now();
        let now_pts = now.saturating_duration_since(self.sync_point);
        let mut shared = self.shared.lock().unwrap();
        self.update_shared(&mut shared, now, now_pts);
        let target = shared.target;
        drop(shared);
        if target.is_none() {
            // not started yet, or reset; the next target is adopted from
            // scratch
            self.anchor = None;
            self.stepped_pts = None;
        }
        target
    }

    /// Start/correction check on the shared state, run by whichever track
    /// calls first.
    fn update_shared(&self, shared: &mut SharedState, now: Instant, now_pts: Duration) {
        let Some(target) = &mut shared.target else {
            shared.target = self.decide_start(&shared.estimator, shared.flushed, now, now_pts);
            return;
        };

        if now.saturating_duration_since(shared.last_check) < CORRECTION_INTERVAL {
            return;
        }
        shared.last_check = now;

        let Some(estimate) = shared.estimator.estimate(now) else {
            return;
        };
        // Newest delivered content relative to the playback position,
        // measured with the actual playback mapping, not the target: while
        // tracks converge the buffer stays out of bounds and the target is
        // refreshed, so the rate keeps tracking the real deviation and falls
        // smoothly as the buffer returns into the band.
        let last_pts = estimate.delivery.last_pts;
        let anchor = self.anchor.unwrap_or(target.anchor);
        let delivered_buffer = anchor.to_output_pts(last_pts).saturating_sub(now_pts);

        let (deviation, bound) = if delivered_buffer > self.options.max_buffer {
            (
                delivered_buffer - self.options.max_buffer,
                self.options.max_buffer,
            )
        } else if delivered_buffer < self.options.min_buffer {
            (
                self.options.min_buffer - delivered_buffer,
                self.options.min_buffer,
            )
        } else {
            return;
        };
        if deviation > RESET_THRESHOLD {
            warn!("Live sync buffer diverged beyond correction, re-estimating the live edge");
            shared.target = None;
            return;
        }
        // rate scales with how far past the bound the buffer is: reaches the
        // cap at twice max_buffer (or at an empty buffer on the other side)
        let severity = f64::min(deviation.div_duration_f64(bound), 1.0);
        *target = CorrectionTarget {
            anchor: TimestampAnchor::new(last_pts, now_pts + self.options.desired_buffer),
            rate: MAX_CORRECTION_RATE * severity,
        };
        debug!(
            ?delivered_buffer,
            ?target,
            "Live sync buffer out of bounds, correcting"
        );
    }

    /// Initial shared target; `None` while the sync should keep buffering.
    fn decide_start(
        &self,
        estimator: &LiveEdgeEstimator,
        flushed: bool,
        now: Instant,
        now_pts: Duration,
    ) -> Option<CorrectionTarget> {
        if flushed {
            let oldest_pts = self
                .buffer
                .front()
                .map(|chunk| chunk.pts)
                .unwrap_or(Duration::ZERO);
            let anchor = TimestampAnchor::new(oldest_pts, now_pts);
            debug!(?anchor, "Live sync started (flush)");
            return Some(CorrectionTarget { anchor, rate: 0.0 });
        }

        let estimate = estimator.estimate(now)?;
        let stable = estimate.upper_bound.stable_for > self.options.stabilization_period;
        let waited_too_long = estimate.delivery.observed_for >= self.options.max_wait;
        if !stable && !waited_too_long {
            return None;
        }
        let newest_buffered_pts = self.buffer.back().map(|chunk| chunk.pts)?;

        // Anchor the newest buffered content `desired_buffer` behind the
        // playback position, so playback starts with exactly the desired
        // buffer; older backlog maps before the start point.
        let anchor = TimestampAnchor::new(
            newest_buffered_pts.saturating_sub(self.options.desired_buffer),
            now_pts,
        );
        debug!(?estimate, ?anchor, "Live sync started");
        Some(CorrectionTarget { anchor, rate: 0.0 })
    }

    /// Moves this track's mapping towards the shared target and returns it.
    /// A step is bounded by the target rate times the content progress since
    /// the last step, so corrections are spread over the content being read
    /// instead of applied at once.
    fn converge_anchor(&mut self, target: CorrectionTarget, pts: Duration) -> TimestampAnchor {
        let Some(anchor) = &mut self.anchor else {
            self.anchor = Some(target.anchor);
            self.stepped_pts = Some(pts);
            return target.anchor;
        };

        let prev = self.stepped_pts.unwrap_or(pts);
        self.stepped_pts = Some(Duration::max(prev, pts));
        let progress = pts.saturating_sub(prev);
        if !progress.is_zero() {
            anchor.converge_towards(&target.anchor, progress.mul_f64(target.rate));
        }
        *anchor
    }
}
