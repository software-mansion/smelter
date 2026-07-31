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
//! a safety valve) by anchoring the newest delivered content `desired_buffer`
//! ahead of the playback position, so playback starts with exactly the
//! desired buffer. Backlog older than that maps before the playback position
//! and plays late or is dropped by the consumer.
//!
//! After the start the buffer (newest delivered content relative to the
//! playback position) is checked against the `min_buffer..max_buffer` band
//! and, while it is outside, corrected back to `desired_buffer`:
//! - The correction is defined once, when it starts: the mapping to converge
//!   to and the rate to converge at. The rate scales with how far past the
//!   band the buffer is - minimal right past it, the full
//!   [`MAX_CORRECTION_RATE`] once the buffer is twice `max_buffer` or empty -
//!   so content is never stretched or squashed by more than that rate (at 4%,
//!   100ms of content plays as 96..104ms).
//! - It is a function of the content timestamp rather than of the wall clock,
//!   so all tracks map the same pts the same way no matter when they read it,
//!   and a correction always runs to completion. A track delayed against the
//!   others applies exactly the correction they applied to the content it
//!   reads instead of jumping to their current mapping ([`Mapping`]).
//! - A buffer diverged so far that correcting it would take minutes is an
//!   anomaly (pts discontinuity, long stall); the sync resets back to the
//!   startup logic instead and the live edge gets re-estimated.

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

    /// Output pts of the next readable chunk; enables interleaved reads
    /// across tracks.
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
    /// Correction starts when the buffer drops below this.
    pub min_buffer: Duration,
    /// Buffer size the sync starts with and corrections converge back to.
    pub desired_buffer: Duration,
    /// Correction starts when the buffer grows beyond this.
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

/// How often the buffer is checked against the band.
const CORRECTION_INTERVAL: Duration = Duration::from_millis(250);
/// Correction rate of a buffer right past the band; slow, but fast enough to
/// finish a correction that a barely exceeded bound can call for.
const MIN_CORRECTION_RATE: f64 = 0.01;
/// Correction rate of a buffer twice `max_buffer` or empty, and the hard cap
/// of how much content time can be stretched or squashed.
const MAX_CORRECTION_RATE: f64 = 0.04;
/// Buffer deviation past the band that is treated as an anomaly (pts
/// discontinuity, long stall) instead of something to correct.
const RESET_THRESHOLD: Duration = Duration::from_secs(10);

/// Synchronization of a single live input; create per-track handles with
/// [`LiveSync::add_track`].
pub(crate) struct LiveSync {
    shared: Arc<Mutex<SharedState>>,
}

impl LiveSync {
    pub fn new(options: LiveSyncOptions, sync_point: Instant) -> Self {
        Self {
            shared: Arc::new(Mutex::new(SharedState {
                estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
                options,
                sync_point,
                flushed: false,
                mapping: None,
                last_check: Instant::now(),
            })),
        }
    }

    /// Registers a new track. All tracks share the live edge detection and
    /// the mapping, so they stay synchronized with each other.
    pub fn add_track(&self) -> LiveSyncTrack {
        LiveSyncTrack {
            shared: self.shared.clone(),
            buffer: VecDeque::new(),
        }
    }

    /// Give up on live edge detection; each track releases everything it
    /// buffered on its next call (e.g. when the stream ended before the live
    /// edge was detected).
    pub fn flush(&self) {
        self.shared.lock().unwrap().flushed = true;
    }
}

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; all synchronization state is shared, a track only
/// owns its buffer.
pub(crate) struct LiveSyncTrack {
    shared: Arc<Mutex<SharedState>>,
    buffer: VecDeque<EncodedInputChunk>,
}

impl LiveSyncTrack {
    pub fn write_chunk(&mut self, chunk: EncodedInputChunk) {
        let now = Instant::now();
        let mut shared = self.shared.lock().unwrap();
        shared.estimator.observe(now, chunk.pts);
        shared.update(now);
        drop(shared);

        self.buffer.push_back(chunk);
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// output timeline; `None` while the live edge is still being detected or
    /// when there is nothing buffered.
    pub fn try_read_chunk(&mut self) -> Option<EncodedInputChunk> {
        let anchor = self.anchor(self.buffer.front()?.pts)?;
        let mut chunk = self.buffer.pop_front()?;
        chunk.pts = anchor.output_pts_of(chunk.pts);
        chunk.dts = chunk.dts.map(|dts| anchor.output_pts_of(dts));
        Some(chunk)
    }

    /// Output pts of the next readable chunk; `None` while the live edge is
    /// still being detected or when nothing is buffered. Enables interleaved
    /// reads across tracks.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        let pts = self.buffer.front()?.pts;
        Some(self.anchor(pts)?.output_pts_of(pts))
    }

    /// Mapping of content at `pts`; `None` while the live edge is still being
    /// detected. Runs the start and correction checks, so they are not tied
    /// to chunks arriving.
    fn anchor(&self, pts: Duration) -> Option<TimestampAnchor> {
        let now = Instant::now();
        let mut shared = self.shared.lock().unwrap();
        shared.update(now);
        Some(shared.mapping?.anchor_at(pts))
    }
}

/// State of an input shared by all of its tracks.
struct SharedState {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    estimator: LiveEdgeEstimator,
    /// Live edge detection was abandoned (e.g. the stream ended before the
    /// edge was detected); tracks release everything they buffered.
    flushed: bool,
    /// Mapping every track applies; `None` until the sync starts and after a
    /// reset.
    mapping: Option<Mapping>,
    /// Last time the buffer was checked against the band.
    last_check: Instant,
}

impl SharedState {
    fn update(&mut self, now: Instant) {
        match self.mapping {
            None => self.try_start(now),
            Some(mapping) => self.try_correct(now, mapping),
        }
    }

    /// Starts the sync once the live edge estimate can be trusted.
    fn try_start(&mut self, now: Instant) {
        let Some(estimate) = self.estimator.estimate(now) else {
            return;
        };
        let reason = if self.flushed {
            "stream ended"
        } else if estimate.upper_bound.stable_for > self.options.stabilization_period {
            "live edge stable"
        } else if estimate.delivery.observed_for >= self.options.max_wait {
            "live edge detection timed out"
        } else {
            return;
        };

        let last_pts = estimate.delivery.last_pts;
        let anchor = self.desired_anchor(now, last_pts);
        debug!(reason, ?anchor, ?estimate, "Live sync started");
        self.mapping = Some(Mapping::started(anchor, last_pts));
    }

    /// Starts a correction when the buffer left the band, unless one is still
    /// running.
    fn try_correct(&mut self, now: Instant, mapping: Mapping) {
        if now.saturating_duration_since(self.last_check) < CORRECTION_INTERVAL {
            return;
        }
        self.last_check = now;

        let Some(estimate) = self.estimator.estimate(now) else {
            return;
        };
        let now_pts = now.saturating_duration_since(self.sync_point);
        // newest delivered content relative to the playback position
        let last_pts = estimate.delivery.last_pts;
        let buffer = mapping
            .anchor_at(last_pts)
            .output_pts_of(last_pts)
            .saturating_sub(now_pts);

        // deviation past the band, and the buffer size at which the full
        // correction rate is reached (twice `max_buffer`, or an empty buffer)
        let (deviation, full_rate_at) = if buffer > self.options.max_buffer {
            (buffer - self.options.max_buffer, self.options.max_buffer)
        } else if buffer < self.options.min_buffer {
            (self.options.min_buffer - buffer, self.options.min_buffer)
        } else {
            return;
        };
        if deviation > RESET_THRESHOLD {
            warn!(
                ?buffer,
                "Live sync buffer diverged beyond correction, re-estimating the live edge"
            );
            self.mapping = None;
            return;
        }
        if !mapping.current.is_settled_at(last_pts) {
            return;
        }

        let severity = f64::min(deviation.div_duration_f64(full_rate_at), 1.0);
        let correction = Correction {
            base: mapping.anchor_at(last_pts),
            target: self.desired_anchor(now, last_pts),
            start_pts: last_pts,
            rate: MIN_CORRECTION_RATE + (MAX_CORRECTION_RATE - MIN_CORRECTION_RATE) * severity,
        };
        debug!(?buffer, ?correction, "Live sync buffer out of bounds");
        self.mapping = Some(mapping.correcting(correction));
    }

    /// Mapping that presents content at `last_pts` `desired_buffer` after the
    /// playback position, i.e. the one that puts the buffer at exactly the
    /// desired size.
    fn desired_anchor(&self, now: Instant, last_pts: Duration) -> TimestampAnchor {
        let now_pts = now.saturating_duration_since(self.sync_point);
        TimestampAnchor::new(last_pts, now_pts + self.options.desired_buffer)
    }
}

/// Mapping of the input timeline onto the output one: the correction in
/// flight, plus the one it replaced.
///
/// The mapping is a function of the content timestamp, not of the wall clock,
/// and starting a correction only ever defines it for content newer than
/// everything mapped so far. Keeping the replaced correction extends that to
/// tracks lagging behind the newest delivered content: they map what they
/// read exactly like the tracks that already passed it, instead of jumping to
/// the newest mapping. Only a track lagging by more than a whole correction
/// falls back to the oldest mapping known, off by at most the correction rate
/// times the lag.
#[derive(Debug, Clone, Copy)]
struct Mapping {
    current: Correction,
    previous: Option<Correction>,
}

impl Mapping {
    fn started(anchor: TimestampAnchor, start_pts: Duration) -> Self {
        Self {
            current: Correction::settled(anchor, start_pts),
            previous: None,
        }
    }

    fn correcting(&self, correction: Correction) -> Self {
        Self {
            current: correction,
            previous: Some(self.current),
        }
    }

    fn anchor_at(&self, pts: Duration) -> TimestampAnchor {
        match self.previous {
            Some(previous) if pts < self.current.start_pts => previous.anchor_at(pts),
            _ => self.current.anchor_at(pts),
        }
    }
}

/// One correction of the mapping: it moves from `base` towards `target` at
/// `rate`, as content past `start_pts` is presented.
#[derive(Debug, Clone, Copy)]
struct Correction {
    /// Mapping of content at `start_pts`.
    base: TimestampAnchor,
    /// Mapping the correction converges to.
    target: TimestampAnchor,
    /// Input pts the correction starts at.
    start_pts: Duration,
    /// Fraction of content time the mapping may be stretched or squashed by.
    rate: f64,
}

impl Correction {
    /// A mapping with nothing left to correct.
    fn settled(anchor: TimestampAnchor, start_pts: Duration) -> Self {
        Self {
            base: anchor,
            target: anchor,
            start_pts,
            rate: 0.0,
        }
    }

    fn anchor_at(&self, pts: Duration) -> TimestampAnchor {
        self.base.converged_towards(self.target, self.slack_at(pts))
    }

    fn is_settled_at(&self, pts: Duration) -> bool {
        self.base.distance(self.target) <= self.slack_at(pts)
    }

    /// How far the mapping may have moved by `pts`.
    fn slack_at(&self, pts: Duration) -> Duration {
        pts.saturating_sub(self.start_pts).mul_f64(self.rate)
    }
}
