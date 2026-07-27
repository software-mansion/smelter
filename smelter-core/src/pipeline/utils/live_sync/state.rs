use std::time::{Duration, Instant};

use tracing::info;

use super::{
    LiveSyncOptions,
    edge_estimator::{LiveEdgeEstimate, LiveEdgeEstimator},
};

/// Correspondence between the input and output timelines chosen when a track
/// starts producing chunks: content at `input_pts` is presented at
/// `output_pts`, and every other timestamp keeps its distance to the anchor.
#[derive(Debug, Clone, Copy)]
pub(super) struct TimestampAnchor {
    /// Raw pts of the anchor: the pts presented right after the start, or the
    /// oldest buffered pts on flush.
    input_pts: Duration,
    /// Pts relative to the sync point at which content at `input_pts` is
    /// presented.
    output_pts: Duration,
}

impl TimestampAnchor {
    /// Maps a raw timestamp (pts or dts) onto the sync point timeline.
    /// Timestamps below `input_pts` (initial backlog) map before the start
    /// point, saturating at zero; such content plays late or is dropped by
    /// the consumer.
    pub(super) fn to_output_pts(&self, pts: Duration) -> Duration {
        (self.output_pts + pts).saturating_sub(self.input_pts)
    }
}

/// Cross-track mutable state of an input, kept behind a mutex.
pub(super) struct SharedState {
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    pub(super) shared_estimator: LiveEdgeEstimator,
    /// Live edge detection was abandoned (e.g. the stream ended before the
    /// edge was detected); tracks release everything they buffered.
    pub(super) flushed: bool,
}

/// Which live edge estimate a track aligned to when it started.
#[derive(Debug, Clone, Copy)]
pub(super) enum EdgeSource {
    Shared,
    Track,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StartDecision {
    pub(super) anchor: TimestampAnchor,
    pub(super) edge: EdgeSource,
}

/// Start decision for a single track based on its own and the shared live
/// edge estimates; `None` while the track should keep buffering.
pub(super) fn decide_start(
    options: &LiveSyncOptions,
    sync_point: Instant,
    now: Instant,
    track_estimate: Option<LiveEdgeEstimate>,
    shared_estimate: Option<LiveEdgeEstimate>,
    flushed: bool,
) -> Option<StartDecision> {
    let output_pts = now.saturating_duration_since(sync_point) + options.start_margin;

    if flushed {
        let min_pts = track_estimate
            .map(|estimate| estimate.min_pts)
            .unwrap_or(Duration::ZERO);
        let anchor = TimestampAnchor {
            input_pts: min_pts,
            output_pts,
        };
        info!(?anchor, "Live sync track started (flush)");
        return Some(StartDecision {
            anchor,
            // the anchor is derived from the track's own data
            edge: EdgeSource::Track,
        });
    }

    let track_estimate = track_estimate?;
    let shared_estimate = shared_estimate?;

    let stable = track_estimate.stable_for >= options.stabilization_period
        && shared_estimate.stable_for >= options.stabilization_period;
    let waited_too_long = track_estimate.observing_for >= options.max_wait;
    let held_too_much =
        track_estimate.max_pts.saturating_sub(track_estimate.min_pts) >= options.max_hold;
    if !stable && !waited_too_long && !held_too_much {
        return None;
    }
    let reason = match (stable, waited_too_long) {
        (true, _) => "live edge stable",
        (false, true) => "live edge detection timed out",
        (false, false) => "buffered content limit reached",
    };

    // The shared estimate is defined by the freshest track; when it lands far
    // away from the track's own estimate, the track lives in an unrelated
    // timestamp space and only its own estimate maps its pts onto the wall
    // clock.
    let edge_distance = shared_estimate.edge_pts.abs_diff(track_estimate.edge_pts);
    let (edge_pts, edge) = match edge_distance < options.shared_edge_tolerance {
        true => (shared_estimate.edge_pts, EdgeSource::Shared),
        false => (track_estimate.edge_pts, EdgeSource::Track),
    };

    // Batched delivery (e.g. HLS segments) needs enough buffer to survive the
    // gap between batches, regardless of the configured buffer size. The gap
    // approximates the batch size; 3/2 leaves headroom for delivery jitter.
    let sustainable_buffer = track_estimate.max_arrival_gap * 3 / 2;
    let target_buffer = options.desired_buffer.max(sustainable_buffer);

    // Content at the chosen edge is presented `start_margin + target_buffer`
    // after "now"; older backlog maps before the start point and is dropped
    // downstream. The mapping depends only on the chosen edge, not on the
    // start moment, so tracks that agree on the shared edge stay in sync
    // even though each starts on its own.
    let anchor_pts = edge_pts
        .saturating_sub(target_buffer)
        .min(track_estimate.max_pts);
    let anchor = TimestampAnchor {
        input_pts: anchor_pts,
        output_pts,
    };
    info!(
        reason,
        ?edge,
        ?edge_distance,
        ?anchor,
        "Live sync track started"
    );
    Some(StartDecision { anchor, edge })
}
