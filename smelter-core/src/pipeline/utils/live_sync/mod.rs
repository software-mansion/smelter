//! Live-edge synchronization for live inputs (RTMP, HLS, MoQ).
//!
//! Live protocols rarely deliver data at a real time rate right after
//! connecting. RTMP clients can flush a few seconds of pre-buffered chunks,
//! HLS delivers whole segments in batches. If playback timing is decided when
//! the connection is established, that initial backlog ends up stretched,
//! squashed or dropped by the queue.
//!
//! [`LiveSync`] represents one input; [`LiveSync::add_track`] returns a
//! [`LiveSyncTrack`] handle per track (video, audio) that buffers chunks
//! written to it. Tracks share the live edge detection and start producing
//! chunks at the same moment, but each handle is independently owned, so
//! tracks can be processed on separate threads.
//!
//! Live edge detection is implemented by [`LiveEdgeEstimator`] (usable on its
//! own by inputs with different buffering logic):
//! - For every chunk it samples `offset = arrival_time - pts`. The smallest
//!   observed offset corresponds to the freshest content seen so far and is
//!   used as the live edge estimate.
//! - When the estimate stops improving for `stabilization_period`, delivery
//!   reached a real time rate and the estimate is considered final. This works
//!   for batched delivery too: the end of each batch is the freshest sample,
//!   so the estimate plateaus between batches regardless of the batch size.
//!
//! Based on the estimate [`LiveSync`] decides how to start:
//!   - Buffer reasonably close to `desired_buffer` (between `min_start_buffer`
//!     and `max_start_buffer`): start with everything that is buffered, no
//!     data is dropped and no extra waiting happens.
//!   - Too much buffered: trim the front (respecting track start points, e.g.
//!     video keyframes) so the steady-state buffer is close to
//!     `desired_buffer`.
//!   - Too little buffered: keep waiting until the buffer grows to
//!     `desired_buffer`.
//! - Safety valves: `max_wait` bounds the total wait time and `max_hold`
//!   bounds the buffered content; when exceeded playback starts immediately
//!   with the current estimate.
//!
//! The configured buffer sizes are treated as floors: for batched delivery
//! the effective sizes are raised to survive the observed gap between batches
//! (`LiveEdgeEstimate::max_arrival_gap`), so the sync works even when the
//! batch size (e.g. HLS segment duration) is unknown or unexpected.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::{debug, info};

mod edge_estimator;
mod item;

pub(crate) use edge_estimator::LiveEdgeEstimator;
pub(crate) use item::LiveSyncItem;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncOptions {
    /// Steady-state buffer (time between a chunk arriving and the moment it is
    /// needed for playback) targeted when the sync trims or waits.
    pub desired_buffer: Duration,
    /// Start is delayed until at least this much content is available.
    pub min_start_buffer: Duration,
    /// Content above this limit is trimmed at start down to `desired_buffer`.
    pub max_start_buffer: Duration,
    /// How long the live edge estimate has to stay stable before starting.
    pub stabilization_period: Duration,
    /// Estimate improvements smaller than this (delivery jitter) do not reset
    /// the stabilization timer.
    pub stabilization_tolerance: Duration,
    /// Extra delay added at start; gives decoders time to process the first
    /// chunks before the queue needs them.
    pub start_margin: Duration,
    /// Start with the current estimate if the live edge was not detected
    /// within this much time from the first chunk.
    pub max_wait: Duration,
    /// Start with the current estimate if more than this much content gets
    /// buffered while waiting for the live edge.
    pub max_hold: Duration,
}

impl LiveSyncOptions {
    pub fn with_desired_buffer(desired_buffer: Duration) -> Self {
        Self {
            desired_buffer,
            min_start_buffer: desired_buffer / 2,
            max_start_buffer: desired_buffer * 3,
            stabilization_period: Duration::from_secs(2),
            stabilization_tolerance: Duration::from_millis(200),
            start_margin: Duration::from_millis(500),
            max_wait: desired_buffer + Duration::from_secs(8),
            max_hold: Duration::from_secs(20).max(desired_buffer * 4),
        }
    }
}

/// Timestamp mapping chosen when the sync starts producing chunks.
///
/// Maps raw input timestamps onto the timeline of the reference instant. When
/// the reference is the queue sync point, mapped timestamps can be sent to a
/// track registered with `QueueTrackOffset::Pts(Duration::ZERO)`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncStart {
    /// Offset from the reference instant at which content at `anchor_pts`
    /// should be presented.
    queue_offset: Duration,
    /// Raw pts that maps to `queue_offset`: the cut point, or the oldest
    /// buffered pts when nothing was trimmed.
    anchor_pts: Duration,
}

impl LiveSyncStart {
    /// Maps a raw timestamp (pts or dts) onto the reference timeline.
    /// Saturating, but effectively always positive: timestamps below
    /// `anchor_pts` (e.g. leading B-frames after a trimmed keyframe) stay
    /// above zero thanks to `start_margin` included in `queue_offset`.
    pub fn to_queue_pts(&self, pts: Duration) -> Duration {
        (self.queue_offset + pts).saturating_sub(self.anchor_pts)
    }
}

/// Synchronization of a single input; create per-track handles with
/// [`LiveSync::add_track`].
pub(crate) struct LiveSync {
    shared: Arc<Shared>,
}

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; only pre-start calls synchronize with other tracks.
pub(crate) struct LiveSyncTrack<T: LiveSyncItem> {
    shared: Arc<Shared>,
    track_index: usize,
    buffer: VecDeque<T>,
    /// Cached shared decision; set once, never changes afterwards.
    start: Option<StartDecision>,
}

struct Shared {
    options: LiveSyncOptions,
    /// Instant that `queue_offset` is measured from (queue sync point).
    sync_point: Instant,
    inner: Mutex<Inner>,
}

struct Inner {
    tracks: Vec<TrackMeta>,
    estimator: LiveEdgeEstimator,
    /// Buffer was too small when the live edge stabilized; waiting until it
    /// grows to the desired size.
    filling: bool,
    start: Option<StartDecision>,
}

#[derive(Default)]
struct TrackMeta {
    /// Track contains items that are not start points (e.g. video with
    /// interframes); such tracks can only be cut at a start point.
    constrained: bool,
    /// pts of start point items observed while buffering.
    start_points: Vec<Duration>,
}

#[derive(Debug, Clone, Copy)]
struct StartDecision {
    info: LiveSyncStart,
    /// Trim each track's buffer to start near this pts; `None` keeps
    /// everything.
    cut_pts: Option<Duration>,
}

impl LiveSync {
    pub fn new(options: LiveSyncOptions, sync_point: Instant) -> Self {
        Self {
            shared: Arc::new(Shared {
                options,
                sync_point,
                inner: Mutex::new(Inner {
                    tracks: Vec::new(),
                    estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
                    filling: false,
                    start: None,
                }),
            }),
        }
    }

    /// Registers a new track. All tracks share the live edge detection and
    /// start at the same moment.
    pub fn add_track<T: LiveSyncItem>(&self) -> LiveSyncTrack<T> {
        let mut inner = self.shared.inner.lock().unwrap();
        inner.tracks.push(TrackMeta::default());
        LiveSyncTrack {
            shared: self.shared.clone(),
            track_index: inner.tracks.len() - 1,
            buffer: VecDeque::new(),
            start: inner.start,
        }
    }

    /// Playback parameters, available once the live edge is detected and
    /// chunks can be read from the tracks.
    pub fn start_info(&self) -> Option<LiveSyncStart> {
        self.shared
            .inner
            .lock()
            .unwrap()
            .start
            .map(|start| start.info)
    }

    /// Give up on live edge detection and release everything that is buffered
    /// (e.g. when the stream ended before the live edge was detected).
    pub fn flush(&self) {
        let mut inner = self.shared.inner.lock().unwrap();
        if inner.start.is_none() {
            inner.start_now(&self.shared, Instant::now(), None, "flush");
        }
    }
}

impl<T: LiveSyncItem> LiveSyncTrack<T> {
    pub fn write_chunk(&mut self, item: T) {
        if self.start.is_some() {
            self.buffer.push_back(item);
            return;
        }
        let now = Instant::now();
        let decision = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.start.is_none() {
                inner.observe(self.track_index, now, item.pts(), item.is_keyframe());
                inner.maybe_start(&self.shared, now);
            }
            inner.start
        };
        self.buffer.push_back(item);
        self.apply_decision(decision);
    }

    /// Returns buffered chunks in write order; `None` while the live edge is
    /// still being detected or when there is nothing buffered.
    pub fn try_read_chunk(&mut self) -> Option<T> {
        if self.start.is_none() {
            let decision = {
                let mut inner = self.shared.inner.lock().unwrap();
                if inner.start.is_none() {
                    inner.maybe_start(&self.shared, Instant::now());
                }
                inner.start
            };
            self.apply_decision(decision);
        }
        match self.start {
            Some(_) => self.buffer.pop_front(),
            None => None,
        }
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

impl Inner {
    fn observe(&mut self, track_index: usize, now: Instant, pts: Duration, is_start_point: bool) {
        let track = &mut self.tracks[track_index];
        match is_start_point {
            true => track.start_points.push(pts),
            false => track.constrained = true,
        }
        self.estimator.observe(now, pts);
    }

    fn maybe_start(&mut self, shared: &Shared, now: Instant) {
        let opts = shared.options;
        if self.start.is_some() {
            return;
        }
        let Some(estimate) = self.estimator.estimate(now) else {
            return;
        };

        let edge_stable = estimate.stable_for >= opts.stabilization_period;
        let waited_too_long = estimate.observing_for >= opts.max_wait;
        let held_too_much = estimate.max_pts.saturating_sub(estimate.min_pts) >= opts.max_hold;
        let forced = waited_too_long || held_too_much;
        if !edge_stable && !forced {
            return;
        }

        // Batched delivery (e.g. HLS segments) needs enough buffer to survive
        // the gap between batches, regardless of the configured buffer sizes.
        // The gap approximates the segment size; 3/2 leaves headroom for
        // delivery jitter.
        let sustainable_buffer = estimate.max_arrival_gap * 3 / 2;
        let min_start_buffer = opts.min_start_buffer.max(sustainable_buffer);
        let desired_buffer = opts.desired_buffer.max(sustainable_buffer);
        let max_start_buffer = opts.max_start_buffer.max(desired_buffer * 2);

        // steady-state buffer if playback starts now from the oldest chunk
        let projected_buffer = estimate.projected_buffer();

        if !forced {
            if projected_buffer < min_start_buffer && !self.filling {
                debug!(
                    "Live edge detected with too little buffered, waiting for the buffer to fill up"
                );
                self.filling = true;
            }
            if self.filling && projected_buffer < desired_buffer {
                return;
            }
        }

        let (cutoff, reason) = if projected_buffer > max_start_buffer {
            let cutoff = estimate
                .edge_pts
                .saturating_sub(desired_buffer)
                .min(estimate.max_pts);
            (Some(cutoff), "trimming excess buffer")
        } else {
            (None, "buffer within limits")
        };
        let reason = match (waited_too_long, held_too_much) {
            (true, _) => "live edge detection timed out",
            (_, true) => "buffered content limit reached",
            _ => reason,
        };
        self.start_now(shared, now, cutoff, reason);
    }

    fn start_now(&mut self, shared: &Shared, now: Instant, cutoff: Option<Duration>, reason: &str) {
        let (cut_pts, anchor_pts) = match cutoff {
            Some(cutoff) => {
                // pts at which every track is able to start; a constrained
                // track with no start point before the cutoff pulls the cut
                // back to its earliest start point (better a bigger buffer
                // than undecodable chunks)
                let cut_pts = self
                    .tracks
                    .iter()
                    .filter(|track| track.constrained)
                    .filter_map(|track| track_cut(track, cutoff))
                    .min()
                    .unwrap_or(cutoff);
                // constrained tracks retain content from their own start
                // point at or before the cut
                let anchor_pts = self
                    .tracks
                    .iter()
                    .filter(|track| track.constrained)
                    .filter_map(|track| track_cut(track, cut_pts))
                    .min()
                    .unwrap_or(cut_pts)
                    .min(cut_pts);
                (Some(cut_pts), anchor_pts)
            }
            None => {
                let min_pts = self
                    .estimator
                    .estimate(now)
                    .map(|estimate| estimate.min_pts);
                (None, min_pts.unwrap_or(Duration::ZERO))
            }
        };
        let queue_offset =
            now.saturating_duration_since(shared.sync_point) + shared.options.start_margin;
        info!(
            reason,
            ?cut_pts,
            ?anchor_pts,
            ?queue_offset,
            "Live sync started"
        );
        self.start = Some(StartDecision {
            info: LiveSyncStart {
                queue_offset,
                anchor_pts,
            },
            cut_pts,
        });
        for track in &mut self.tracks {
            track.start_points = Vec::new();
        }
    }
}

/// Latest start point of the track at or before `at`, falling back to the
/// track's earliest start point.
fn track_cut(track: &TrackMeta, at: Duration) -> Option<Duration> {
    track
        .start_points
        .iter()
        .copied()
        .filter(|pts| *pts <= at)
        .max()
        .or_else(|| track.start_points.iter().copied().min())
}
