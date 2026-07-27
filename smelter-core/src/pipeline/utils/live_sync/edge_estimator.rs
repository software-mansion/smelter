use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Only arrival gaps observed within the last two windows of this length
/// affect [`LiveEdgeEstimator::max_arrival_gap`]; a one-off stall stops
/// inflating the value once the windows rotate past it.
const ARRIVAL_GAP_WINDOW: Duration = Duration::from_secs(30);

/// Bucket size of the windowed offset estimates.
const EDGE_WINDOW_BUCKET: Duration = Duration::from_secs(10);
/// Number of recent buckets contributing to the estimates; the look-back is
/// between one and this many buckets.
const EDGE_WINDOW_BUCKETS: u64 = 6;

/// Estimates the live edge of a stream by observing chunk arrival times.
///
/// For every chunk it samples `offset = arrival_time - pts` and tracks the
/// extremes over a recent window; extrapolating from them bounds the edge
/// ([`EdgeBounds`]). The minimum offset is the floor of the delivery delay
/// (the same technique as LEDBAT base delay or BBR min_rtt) and yields the
/// upper bound; the maximum offset yields the lower bound. Real production
/// time is not observable from one-way arrivals; it can only be bounded like
/// this. Once the minimum offset settles ([`EdgeBounds::stable_for`]),
/// delivery reached a real time rate and the bounds can be trusted.
///
/// The window ([`EDGE_WINDOW_BUCKET`] * [`EDGE_WINDOW_BUCKETS`] look-back)
/// makes the bounds follow changes of the network latency instead of locking
/// to lifetime extremes; it also means they react to such changes with up to
/// a window of delay. Fast reactions (e.g. buffer corrections) should
/// compare the playback position against delivered content
/// ([`LiveEdgeEstimator::max_pts`]) instead.
///
/// Next to the bounds the estimator exposes plain delivery statistics;
/// unlike the bounds they describe what was actually observed and do not
/// extrapolate.
///
/// Only the detection lives here; what to do with the estimate (buffering,
/// anchoring, offset slewing) is up to the caller.
pub(crate) struct LiveEdgeEstimator {
    /// Instant that pts values are compared against.
    sync_point: Instant,
    /// Offset moves smaller than this (delivery jitter) do not reset the
    /// stability timer.
    tolerance: Duration,
    observations: Option<Observations>,
}

struct Observations {
    first_observation: Instant,
    last_observation: Instant,
    min_pts: Duration,
    max_pts: Duration,
    /// Per-bucket extremes of `arrival - pts` (in nanoseconds relative to
    /// the sync point, negative when pts run ahead of the wall clock),
    /// oldest first; never empty.
    offsets: VecDeque<OffsetBucket>,
    /// Windowed minimum offset when the stability timer was last reset.
    stable_offset_ns: i64,
    stable_since: Instant,
    gaps: GapWindow,
}

struct OffsetBucket {
    index: u64,
    min_offset_ns: i64,
    max_offset_ns: i64,
}

impl Observations {
    /// Offset extremes `(min, max)` over the buckets still within the window;
    /// falls back to the newest bucket when nothing was observed within the
    /// window (prolonged stall), so the last known regime keeps being
    /// reported.
    fn offset_bounds_ns(&self, oldest_valid_bucket: u64) -> (i64, i64) {
        let mut bounds = None;
        for bucket in &self.offsets {
            if bucket.index < oldest_valid_bucket {
                continue;
            }
            bounds = Some(match bounds {
                None => (bucket.min_offset_ns, bucket.max_offset_ns),
                Some((min, max)) => (
                    i64::min(min, bucket.min_offset_ns),
                    i64::max(max, bucket.max_offset_ns),
                ),
            });
        }
        bounds.unwrap_or_else(|| {
            let newest = self.offsets.back().expect("never empty");
            (newest.min_offset_ns, newest.max_offset_ns)
        })
    }
}

/// Coarse sliding-window maximum of arrival gaps: gaps collect into the
/// current window and the previous window still counts, so the effective
/// look-back is between one and two [`ARRIVAL_GAP_WINDOW`]s.
struct GapWindow {
    started_at: Instant,
    current_max: Duration,
    previous_max: Duration,
}

impl GapWindow {
    fn record(&mut self, now: Instant, gap: Duration) {
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed >= ARRIVAL_GAP_WINDOW * 2 {
            self.previous_max = Duration::ZERO;
            self.current_max = Duration::ZERO;
            self.started_at = now;
        } else if elapsed >= ARRIVAL_GAP_WINDOW {
            self.previous_max = self.current_max;
            self.current_max = Duration::ZERO;
            self.started_at = now;
        }
        self.current_max = self.current_max.max(gap);
    }

    fn max(&self) -> Duration {
        self.current_max.max(self.previous_max)
    }
}

/// Bounds on the live edge at a specific instant, extrapolated from the
/// extremes of the recently observed delivery delay.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EdgeBounds {
    /// pts that would be arriving right now if delivery was as fast as the
    /// fastest recent chunk; delivery cannot be past it (and possibly has
    /// not reached it yet, e.g. between HLS segments).
    pub upper: Duration,
    /// pts that would be arriving right now if delivery was as slow as the
    /// slowest recent chunk; content up to it should have arrived already.
    pub lower: Duration,
    /// How long the windowed minimum offset (the `upper` bound) has not
    /// improved beyond the jitter tolerance. Faster-than-real-time delivery
    /// (a backlog flush) keeps improving it, so a plateau means delivery
    /// reached a real time rate. One-sided: the floor rising (delivery
    /// worsening) does not reset the timer.
    pub stable_for: Duration,
}

impl LiveEdgeEstimator {
    pub fn new(sync_point: Instant, tolerance: Duration) -> Self {
        Self {
            sync_point,
            tolerance,
            observations: None,
        }
    }

    /// Record a chunk with `pts` that arrived at `now`.
    pub fn observe(&mut self, now: Instant, pts: Duration) {
        let arrival_ns = signed_ns(now.saturating_duration_since(self.sync_point));
        let offset_ns = arrival_ns - signed_ns(pts);
        let bucket_index = self.bucket_index(now);
        let tolerance_ns = signed_ns(self.tolerance);
        match &mut self.observations {
            None => {
                self.observations = Some(Observations {
                    first_observation: now,
                    last_observation: now,
                    min_pts: pts,
                    max_pts: pts,
                    offsets: VecDeque::from([OffsetBucket {
                        index: bucket_index,
                        min_offset_ns: offset_ns,
                        max_offset_ns: offset_ns,
                    }]),
                    stable_offset_ns: offset_ns,
                    stable_since: now,
                    gaps: GapWindow {
                        started_at: now,
                        current_max: Duration::ZERO,
                        previous_max: Duration::ZERO,
                    },
                })
            }
            Some(observations) => {
                let gap = now.saturating_duration_since(observations.last_observation);
                observations.gaps.record(now, gap);
                observations.last_observation = now;
                observations.min_pts = observations.min_pts.min(pts);
                observations.max_pts = observations.max_pts.max(pts);

                match observations.offsets.back_mut() {
                    Some(bucket) if bucket.index == bucket_index => {
                        bucket.min_offset_ns = bucket.min_offset_ns.min(offset_ns);
                        bucket.max_offset_ns = bucket.max_offset_ns.max(offset_ns);
                    }
                    _ => observations.offsets.push_back(OffsetBucket {
                        index: bucket_index,
                        min_offset_ns: offset_ns,
                        max_offset_ns: offset_ns,
                    }),
                }
                let oldest_valid = bucket_index.saturating_sub(EDGE_WINDOW_BUCKETS - 1);
                while observations.offsets.len() > 1
                    && observations
                        .offsets
                        .front()
                        .is_some_and(|bucket| bucket.index < oldest_valid)
                {
                    observations.offsets.pop_front();
                }

                // One-sided: only an improvement (fresher delivery, so the
                // edge was not reached yet) restarts the timer. The floor
                // rising (old minimum rotating out of the window) re-bases
                // the reference so later improvements are measured against
                // the current level, but does not reset stability.
                let (current_offset_ns, _) = observations.offset_bounds_ns(oldest_valid);
                if observations.stable_offset_ns - current_offset_ns > tolerance_ns {
                    observations.stable_offset_ns = current_offset_ns;
                    observations.stable_since = now;
                } else if current_offset_ns > observations.stable_offset_ns {
                    observations.stable_offset_ns = current_offset_ns;
                }
            }
        }
    }

    /// `None` until the first observation.
    pub fn edge_bounds(&self, now: Instant) -> Option<EdgeBounds> {
        let observations = self.observations.as_ref()?;
        let oldest_valid = self
            .bucket_index(now)
            .saturating_sub(EDGE_WINDOW_BUCKETS - 1);
        let (min_offset_ns, max_offset_ns) = observations.offset_bounds_ns(oldest_valid);
        let now_ns = signed_ns(now.saturating_duration_since(self.sync_point));
        Some(EdgeBounds {
            // negative only when pts run ahead of the wall clock; saturate,
            // the estimate is meaningless for such streams anyway
            upper: Duration::from_nanos((now_ns - min_offset_ns).max(0) as u64),
            lower: Duration::from_nanos((now_ns - max_offset_ns).max(0) as u64),
            stable_for: now.saturating_duration_since(observations.stable_since),
        })
    }

    /// Time since the first observed chunk.
    pub fn observing_for(&self, now: Instant) -> Option<Duration> {
        let observations = self.observations.as_ref()?;
        Some(now.saturating_duration_since(observations.first_observation))
    }

    /// Smallest observed pts.
    pub fn min_pts(&self) -> Option<Duration> {
        self.observations
            .as_ref()
            .map(|observations| observations.min_pts)
    }

    /// Largest observed pts; the newest delivered content.
    pub fn max_pts(&self) -> Option<Duration> {
        self.observations
            .as_ref()
            .map(|observations| observations.max_pts)
    }

    /// Largest recent gap between consecutive chunk arrivals (including the
    /// one in progress). Close to zero for continuous delivery; approximates
    /// the segment interval for batched delivery like HLS.
    pub fn max_arrival_gap(&self, now: Instant) -> Option<Duration> {
        let observations = self.observations.as_ref()?;
        let in_progress = now.saturating_duration_since(observations.last_observation);
        Some(observations.gaps.max().max(in_progress))
    }

    fn bucket_index(&self, at: Instant) -> u64 {
        (at.saturating_duration_since(self.sync_point).as_nanos() / EDGE_WINDOW_BUCKET.as_nanos())
            as u64
    }
}

fn signed_ns(duration: Duration) -> i64 {
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}
