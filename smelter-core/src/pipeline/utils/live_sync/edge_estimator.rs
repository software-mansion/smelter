use std::time::{Duration, Instant};

/// Estimates the live edge of a stream by observing chunk arrival times.
///
/// For every chunk it samples `offset = arrival_time - pts`. The smallest
/// observed offset corresponds to the freshest content seen so far and is used
/// as the live edge estimate. Once the estimate stops improving, delivery
/// reached a real time rate and content at `edge_pts` is being produced by the
/// source right now.
///
/// Only the detection lives here; what to do with the estimate (buffering,
/// trimming, offset slewing) is up to the caller.
pub(crate) struct LiveEdgeEstimator {
    /// Instant that pts values are compared against.
    reference: Instant,
    /// Estimate improvements smaller than this (delivery jitter) do not reset
    /// the stability timer.
    tolerance: Duration,
    observations: Option<Observations>,
}

struct Observations {
    first_observation: Instant,
    last_observation: Instant,
    max_arrival_gap: Duration,
    min_pts: Duration,
    max_pts: Duration,
    /// Smallest observed `arrival - pts` (in nanoseconds relative to the
    /// reference instant); the live edge estimate.
    edge_offset_ns: i64,
    /// Value of `edge_offset_ns` when the stability timer was last reset.
    stable_edge_offset_ns: i64,
    stable_since: Instant,
}

/// Snapshot of the live edge estimate at a specific instant.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveEdgeEstimate {
    /// Time since the first observed chunk.
    pub observing_for: Duration,
    /// How long the estimate has not improved (beyond the jitter tolerance).
    pub stable_for: Duration,
    /// pts of the content that the source is producing right now (possibly
    /// not delivered yet, e.g. between HLS segments).
    pub edge_pts: Duration,
    /// Largest gap between consecutive chunk arrivals (including the one in
    /// progress). Close to zero for continuous delivery; approximates the
    /// segment interval for batched delivery like HLS.
    pub max_arrival_gap: Duration,
    /// Smallest observed pts.
    pub min_pts: Duration,
    /// Largest observed pts.
    pub max_pts: Duration,
}

impl LiveEdgeEstimate {
    /// Steady-state buffer if playback starts right now from the oldest
    /// observed pts.
    pub fn projected_buffer(&self) -> Duration {
        self.edge_pts.saturating_sub(self.min_pts)
    }
}

impl LiveEdgeEstimator {
    pub fn new(reference: Instant, tolerance: Duration) -> Self {
        Self {
            reference,
            tolerance,
            observations: None,
        }
    }

    /// Record a chunk with `pts` that arrived at `now`.
    pub fn observe(&mut self, now: Instant, pts: Duration) {
        let arrival_ns = signed_ns(now.saturating_duration_since(self.reference));
        let offset_ns = arrival_ns - signed_ns(pts);
        match &mut self.observations {
            None => {
                self.observations = Some(Observations {
                    first_observation: now,
                    last_observation: now,
                    max_arrival_gap: Duration::ZERO,
                    min_pts: pts,
                    max_pts: pts,
                    edge_offset_ns: offset_ns,
                    stable_edge_offset_ns: offset_ns,
                    stable_since: now,
                })
            }
            Some(observations) => {
                let arrival_gap = now.saturating_duration_since(observations.last_observation);
                observations.max_arrival_gap = observations.max_arrival_gap.max(arrival_gap);
                observations.last_observation = now;
                observations.min_pts = observations.min_pts.min(pts);
                observations.max_pts = observations.max_pts.max(pts);
                observations.edge_offset_ns = observations.edge_offset_ns.min(offset_ns);
                let improvement = observations.stable_edge_offset_ns - observations.edge_offset_ns;
                if improvement > signed_ns(self.tolerance) {
                    observations.stable_edge_offset_ns = observations.edge_offset_ns;
                    observations.stable_since = now;
                }
            }
        }
    }

    /// `None` until the first observation.
    pub fn estimate(&self, now: Instant) -> Option<LiveEdgeEstimate> {
        let observations = self.observations.as_ref()?;
        let now_ns = signed_ns(now.saturating_duration_since(self.reference));
        let edge_pts_ns = now_ns - observations.edge_offset_ns;
        Some(LiveEdgeEstimate {
            observing_for: now.saturating_duration_since(observations.first_observation),
            stable_for: now.saturating_duration_since(observations.stable_since),
            // negative only when pts run ahead of the wall clock; saturate,
            // the estimate is meaningless for such streams anyway
            edge_pts: Duration::from_nanos(edge_pts_ns.max(0) as u64),
            max_arrival_gap: observations
                .max_arrival_gap
                .max(now.saturating_duration_since(observations.last_observation)),
            min_pts: observations.min_pts,
            max_pts: observations.max_pts,
        })
    }
}

fn signed_ns(duration: Duration) -> i64 {
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}
