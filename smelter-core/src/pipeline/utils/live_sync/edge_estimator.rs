use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Width of one bucket; the granularity at which old observations expire.
const BUCKET: Duration = Duration::from_secs(2);

/// Offset window look-back for perfectly steady delivery; observed arrival
/// gaps only grow it from here.
const MIN_LOOKBACK: Duration = Duration::from_secs(20);

/// Hard cap of the offset window look-back and of the retained history.
const MAX_LOOKBACK: Duration = Duration::from_secs(120);

/// The offset window covers at least this many worst-case arrival gaps, so
/// batched delivery (e.g. HLS segments) keeps several bursts in view.
const LOOKBACK_GAPS: u32 = 4;

/// Look-back of the arrival gap maximum. Longer than [`MIN_LOOKBACK`], so a
/// one-off stall keeps the offset window widened for a while after it ends
/// instead of being forgotten as soon as it leaves a short window.
const GAP_LOOKBACK: Duration = Duration::from_secs(60);

/// Continuously estimates the live edge of a stream by observing chunk
/// arrival times.
///
/// For every chunk it samples `offset = arrival_time - pts`. Real production
/// time is not observable from one-way arrivals, so the edge can only be
/// bounded by the recent extremes of the offset:
/// - the minimum (the fastest delivery seen; the same technique as LEDBAT
///   base delay or BBR min_rtt) extrapolates to
///   [`EdgeEstimate::upper_bound`],
/// - the maximum (the slowest delivery seen) to
///   [`EdgeEstimate::lower_bound`].
///
/// An estimate is available from the first observation on and can be queried
/// at any time; it is always the best knowable at that moment, and
/// [`PtsBound::stable_for`] tells how much to trust each bound (a backlog
/// flush right after connecting keeps extending the upper bound until
/// delivery drops to a real time rate).
///
/// The extremes are tracked per bucket over a sliding window, so the estimate
/// follows changes of the network latency instead of locking to lifetime
/// extremes. The look-back adapts to the observed delivery pattern: it stays
/// at [`MIN_LOOKBACK`] for continuous streams (WebRTC, RTMP) and stretches to
/// cover several arrival gaps for batched ones (HLS segments), up to
/// [`MAX_LOOKBACK`]. Adaptation to a latency change therefore takes up to one
/// window; fast reactions (e.g. buffer corrections) should compare the
/// playback position against delivered content ([`DeliveryStats::last_pts`])
/// instead.
///
/// Only the estimation lives here; what to do with it (buffering, anchoring,
/// offset slewing) is up to the caller.
pub(crate) struct LiveEdgeEstimator {
    /// Instant that pts values are compared against.
    sync_point: Instant,
    /// Bound extensions smaller than this (delivery jitter) do not reset the
    /// stability timers.
    tolerance: Duration,
    observations: Option<Observations>,
}

struct Observations {
    first_observation: Instant,
    last_observation: Instant,
    last_pts: Duration,
    /// Per-bucket aggregates, oldest first; never empty. Offsets are
    /// `arrival - pts` in nanoseconds relative to the sync point, negative
    /// when pts run ahead of the wall clock.
    buckets: VecDeque<Bucket>,
    min_offset_stability: Stability,
    max_offset_stability: Stability,
}

/// Snapshot of the live edge estimate at a specific instant.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EdgeEstimate {
    /// Optimistic bound. Pts that would be arriving right now if delivery was
    /// as fast as the fastest recent chunk.
    ///
    /// What we know from that value:
    /// - If we want to maintain buffer in range (MIN, MAX), then upper bound can be
    ///   used to estimate if we did not buffer too much. However check for lower bound has higher
    ///   priority.
    /// - For protocols with large chunks it represents the end of the chunk.
    /// - If upper_bound is stable it grows like real-time or slower. (It is still stable if it grows
    ///   faster within tolerance range.)
    ///   - It can grow faster than real time only on new data (assuming fixed window, e.g. gaps can affect it)
    ///   - It can grow slower than real time only when "forgetting" (assuming fixed window, e.g. gaps can affect it)
    ///   - At the start stability can mean finding the edge
    ///   - For chunks a lot larger than stability period, it will stabilize because no data
    ///     for stability period means that upper bound can't increase.
    ///   - For chunks slightly smaller than stability period. After the chunk upper bound
    ///     is calculated from offset that was established on last part of the chunk. So as
    ///     long distance between arrivals of end of chunks is similar to chunk size it will
    ///     work
    ///   - For chunks a lot smaller than stability period it obviously works (the simplest case)
    pub upper_bound: PtsBound,
    /// Pessimistic bound. Pts that would be arriving right now if delivery
    /// was as slow as the slowest recent chunk.
    ///
    /// What we know from that value:
    /// - If we want to maintain buffer in range (MIN, MAX), then lower bound can be
    ///   used to estimate if we did not buffer too little
    /// - For protocols with large chunks it represents start of the chunk
    /// - If lower_bound is stable it grows like real-time or faster. (It is still stable if it grows
    ///   slower within tolerance range)
    ///   - It can grow slower than real time only on new data (assuming fixed window, e.g. gaps can affect it)
    ///   - It can grow faster than real time only when "forgetting" (assuming fixed window, e.g. gaps can affect it)
    ///   - Breaking stable state might mean degraded performance, but it is too late to treat it as
    ///     a signal because it is set after late packet arrives
    ///   - TODO: If we estimate gap size (equal to chunk size) we could lower this bound without
    ///     new incoming packet (when packet we expect was not yet delivered)
    pub lower_bound: PtsBound,
    /// Plain statistics of what was actually delivered; unlike the bounds
    /// they do not extrapolate.
    pub delivery: DeliveryStats,
}

/// One side of the live edge estimate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PtsBound {
    pub pts: Duration,
    /// How long the bound has not been pushed outward / relaxed (beyond the
    /// jitter tolerance) by an observed chunk. The bound tightening as old
    /// extremes rotate out of the window does not reset it.
    pub stable_for: Duration,
}

/// Observed delivery statistics of the stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeliveryStats {
    /// Largest observed pts (not the last received one, so decode order does
    /// not matter); the newest delivered content.
    pub last_pts: Duration,
    /// Time since the first observed chunk.
    pub observed_for: Duration,
    /// Largest recent gap between consecutive chunk arrivals (including the
    /// one in progress). Close to zero for continuous delivery; approximates
    /// the segment interval for batched delivery like HLS.
    #[allow(dead_code)]
    pub max_arrival_gap: Duration,
    /// Time since the newest observed arrival; the arrival gap currently in
    /// progress.
    pub since_last_arrival: Duration,
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
        let offset_ns = arrival_ns.saturating_sub(signed_ns(pts));
        let now_index = self.bucket_index(now);

        let Some(observations) = &mut self.observations else {
            self.observations = Some(Observations {
                first_observation: now,
                last_observation: now,
                last_pts: pts,
                buckets: VecDeque::from([Bucket {
                    index: now_index,
                    min_offset_ns: offset_ns,
                    max_offset_ns: offset_ns,
                    max_gap: Duration::ZERO,
                }]),
                min_offset_stability: Stability::new(Extreme::Min, now, offset_ns),
                max_offset_stability: Stability::new(Extreme::Max, now, offset_ns),
            });
            return;
        };

        observations.record(now, now_index, offset_ns, pts);

        let max_gap = observations.max_arrival_gap(now, now_index);
        let (min_offset_ns, max_offset_ns) = observations.offset_bounds_ns(now_index, max_gap);
        let tolerance_ns = signed_ns(self.tolerance);
        observations
            .min_offset_stability
            .track(now, min_offset_ns, tolerance_ns);
        observations
            .max_offset_stability
            .track(now, max_offset_ns, tolerance_ns);
    }

    /// Current estimate; `None` until the first observation.
    pub fn estimate(&self, now: Instant) -> Option<EdgeEstimate> {
        let observations = self.observations.as_ref()?;
        let now_index = self.bucket_index(now);
        let max_gap = observations.max_arrival_gap(now, now_index);
        let (min_offset_ns, max_offset_ns) = observations.offset_bounds_ns(now_index, max_gap);
        let now_ns = signed_ns(now.saturating_duration_since(self.sync_point));
        Some(EdgeEstimate {
            // pts saturate at zero: negative only when pts run ahead of the
            // wall clock, and the estimate is meaningless for such streams
            // anyway
            upper_bound: PtsBound {
                pts: Duration::from_nanos(i64::max(now_ns.saturating_sub(min_offset_ns), 0) as u64),
                stable_for: now.saturating_duration_since(observations.min_offset_stability.since),
            },
            lower_bound: PtsBound {
                pts: Duration::from_nanos(i64::max(now_ns.saturating_sub(max_offset_ns), 0) as u64),
                stable_for: now.saturating_duration_since(observations.max_offset_stability.since),
            },
            delivery: DeliveryStats {
                last_pts: observations.last_pts,
                observed_for: now.saturating_duration_since(observations.first_observation),
                max_arrival_gap: max_gap,
                since_last_arrival: now.saturating_duration_since(observations.last_observation),
            },
        })
    }

    fn bucket_index(&self, at: Instant) -> u64 {
        (at.saturating_duration_since(self.sync_point).as_nanos() / BUCKET.as_nanos()) as u64
    }
}

fn buckets_in(window: Duration) -> u64 {
    window.as_nanos().div_ceil(BUCKET.as_nanos()) as u64
}

fn signed_ns(duration: Duration) -> i64 {
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}

/// Which offset extreme a [`Stability`] tracker follows, i.e. which direction
/// counts as extending.
enum Extreme {
    /// Offset minimum; extends downward. Backs the upper pts bound.
    Min,
    /// Offset maximum; extends upward. Backs the lower pts bound.
    Max,
}

/// One-sided stability tracking of one offset extreme: how long since the
/// extreme was last extended (i.e. revealed new information about delivery).
struct Stability {
    extreme: Extreme,
    /// Extreme value when the timer was last reset.
    reference_ns: i64,
    since: Instant,
}

impl Stability {
    fn new(extreme: Extreme, now: Instant, offset_ns: i64) -> Self {
        Self {
            extreme,
            reference_ns: offset_ns,
            since: now,
        }
    }

    /// Only an extension beyond the tolerance resets the timer; a tightening
    /// (the old extreme rotating out of the window) re-bases the reference so
    /// later extensions are measured against the current level, but does not
    /// reset stability.
    fn track(&mut self, now: Instant, current_ns: i64, tolerance_ns: i64) {
        let extension_ns = match self.extreme {
            Extreme::Min => self.reference_ns.saturating_sub(current_ns),
            Extreme::Max => current_ns.saturating_sub(self.reference_ns),
        };
        if extension_ns > tolerance_ns {
            self.reference_ns = current_ns;
            self.since = now;
        } else if extension_ns < 0 {
            self.reference_ns = current_ns;
        }
    }
}

struct Bucket {
    index: u64,
    min_offset_ns: i64,
    max_offset_ns: i64,
    /// Largest gap between consecutive arrivals, attributed to the bucket
    /// where the gap ended.
    max_gap: Duration,
}

impl Observations {
    /// Record one chunk: update delivery stats, fold the offset into the
    /// bucket for `now_index` and trim buckets beyond the retention window.
    fn record(&mut self, now: Instant, now_index: u64, offset_ns: i64, pts: Duration) {
        let gap = now.saturating_duration_since(self.last_observation);
        self.last_observation = now;
        self.last_pts = Duration::max(self.last_pts, pts);

        match self.buckets.back_mut() {
            Some(bucket) if bucket.index == now_index => {
                bucket.min_offset_ns = i64::min(bucket.min_offset_ns, offset_ns);
                bucket.max_offset_ns = i64::max(bucket.max_offset_ns, offset_ns);
                bucket.max_gap = Duration::max(bucket.max_gap, gap);
            }
            _ => self.buckets.push_back(Bucket {
                index: now_index,
                min_offset_ns: offset_ns,
                max_offset_ns: offset_ns,
                max_gap: gap,
            }),
        }
        let oldest_retained = now_index.saturating_sub(buckets_in(MAX_LOOKBACK) - 1);
        while self.buckets.len() > 1
            && self
                .buckets
                .front()
                .is_some_and(|bucket| bucket.index < oldest_retained)
        {
            self.buckets.pop_front();
        }
    }

    /// Largest recent gap between consecutive arrivals, including the one in
    /// progress at `now`.
    fn max_arrival_gap(&self, now: Instant, now_index: u64) -> Duration {
        let oldest_valid = now_index.saturating_sub(buckets_in(GAP_LOOKBACK) - 1);
        // seeded with the gap in progress: an ongoing stall has no bucket yet
        let mut max_gap = now.saturating_duration_since(self.last_observation);
        for bucket in &self.buckets {
            if bucket.index >= oldest_valid {
                max_gap = Duration::max(max_gap, bucket.max_gap);
            }
        }
        max_gap
    }

    /// Offset extremes `(min, max)` over the window; the look-back scales
    /// with the arrival gap so batched delivery keeps several bursts in
    /// view. Falls back to the newest bucket when nothing was observed
    /// within the window (prolonged stall), so the last known regime keeps
    /// being reported.
    fn offset_bounds_ns(&self, now_index: u64, max_gap: Duration) -> (i64, i64) {
        let lookback = Duration::clamp(max_gap * LOOKBACK_GAPS, MIN_LOOKBACK, MAX_LOOKBACK);
        let oldest_valid = now_index.saturating_sub(buckets_in(lookback) - 1);
        let mut bounds = None;
        for bucket in &self.buckets {
            if bucket.index < oldest_valid {
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
            let newest = self.buckets.back().expect("never empty");
            (newest.min_offset_ns, newest.max_offset_ns)
        })
    }
}
