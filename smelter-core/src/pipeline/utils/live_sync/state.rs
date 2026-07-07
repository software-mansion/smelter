use std::time::{Duration, Instant};

use super::{LiveSyncOptions, edge_estimator::LiveEdgeEstimator};
use crate::pipeline::utils::input_sync::TimestampAnchor;

/// Cross-track mutable state of an input, kept behind a mutex.
pub(super) struct SharedState {
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    pub(super) shared_estimator: LiveEdgeEstimator,
    pub(super) anchor: Option<TimestampAnchor>,
}

/// Which live edge estimate a track aligned to when it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EdgeSource {
    Shared,
    Track,
}

pub(super) fn resolve_should_start(
    now: Instant,
    opts: &LiveSyncOptions,
    track_estimator: &LiveEdgeEstimator,
    shared_estimator: &LiveEdgeEstimator,
) -> Option<EdgeSource> {
    let shared = shared_estimator.estimate(now)?;
    let track = track_estimator.estimate(now)?;
    // TODO: consider what should happen if only one tracks is producing packets, and
    // at some point, other track will start

    let track_stable = track.upper_bound.stable_for > opts.stabilization_period;
    let shared_stable = shared.upper_bound.stable_for > opts.stabilization_period;
    let waiting_too_long = shared.delivery.observed_for >= opts.max_wait;

    // check if track and shared estimator are desynced. If for some reason track have independent
    // timescales, fallback to independent edge estimation
    let upper_diff = Duration::abs_diff(track.upper_bound.pts, shared.upper_bound.pts);
    let lower_diff = Duration::abs_diff(track.lower_bound.pts, shared.lower_bound.pts);
    let tracks_desynced =
        upper_diff > Duration::from_secs(10) || lower_diff > Duration::from_secs(10);

    if track_stable && shared_stable {
        return match tracks_desynced {
            true => Some(EdgeSource::Track),
            false => Some(EdgeSource::Shared),
        };
    }

    if !waiting_too_long {
        return None;
    }

    return match tracks_desynced {
        true => Some(EdgeSource::Track),
        false => Some(EdgeSource::Shared),
    };
}
