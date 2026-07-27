use std::time::{Duration, Instant};

use tracing::info;

use super::{
    LiveSyncOptions,
    edge_estimator::{EdgeBounds, LiveEdgeEstimator},
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

    /// Shifts the mapping so content is presented `delta` earlier.
    pub(super) fn shift_earlier(&mut self, delta: Duration) {
        self.input_pts += delta;
    }

    /// Shifts the mapping so content is presented `delta` later.
    pub(super) fn shift_later(&mut self, delta: Duration) {
        self.output_pts += delta;
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
    /// Delivery offset (`elapsed - edge pts`) of the chosen edge at the start;
    /// reference for detecting that the edge improved after the start (the
    /// start was based on a false knee, e.g. a stall mistaken for the edge).
    pub(super) edge_offset: Duration,
}

/// Start decision for a single track based on its own estimator and the
/// shared edge bounds; `None` while the track should keep buffering.
pub(super) fn decide_start(
    options: &LiveSyncOptions,
    sync_point: Instant,
    now: Instant,
    estimator: &LiveEdgeEstimator,
    shared_bounds: Option<EdgeBounds>,
    flushed: bool,
) -> Option<StartDecision> {
    let elapsed = now.saturating_duration_since(sync_point);
    let output_pts = elapsed + options.start_margin;

    if flushed {
        let min_pts = estimator.min_pts().unwrap_or(Duration::ZERO);
        let anchor = TimestampAnchor {
            input_pts: min_pts,
            output_pts,
        };
        info!(?anchor, "Live sync track started (flush)");
        return Some(StartDecision {
            anchor,
            // the anchor is derived from the track's own data
            edge: EdgeSource::Track,
            edge_offset: elapsed.saturating_sub(min_pts),
        });
    }

    let track_bounds = estimator.edge_bounds(now)?;
    let shared_bounds = shared_bounds?;
    let max_pts = estimator.max_pts()?;
    let min_pts = estimator.min_pts()?;

    let stable = track_bounds.stable_for >= options.stabilization_period
        && shared_bounds.stable_for >= options.stabilization_period;
    let waited_too_long = estimator.observing_for(now)? >= options.max_wait;
    let held_too_much = max_pts.saturating_sub(min_pts) >= options.max_hold;
    if !stable && !waited_too_long && !held_too_much {
        return None;
    }
    let reason = match (stable, waited_too_long) {
        (true, _) => "live edge stable",
        (false, true) => "live edge detection timed out",
        (false, false) => "buffered content limit reached",
    };

    // The shared upper edge is defined by the freshest track; when it lands
    // far away from the track's own, the track lives in an unrelated
    // timestamp space and only its own edge maps its pts onto the wall
    // clock.
    let edge_distance = shared_bounds.upper.abs_diff(track_bounds.upper);
    let (edge_pts, edge) = match edge_distance < options.shared_edge_tolerance {
        true => (shared_bounds.upper, EdgeSource::Shared),
        false => (track_bounds.upper, EdgeSource::Track),
    };

    // Batched delivery (e.g. HLS segments) needs enough buffer to survive the
    // gap between batches, regardless of the configured buffer size. The gap
    // approximates the batch size; 3/2 leaves headroom for delivery jitter.
    let sustainable_buffer = estimator.max_arrival_gap(now)? * 3 / 2;
    let target_buffer = options.desired_buffer.max(sustainable_buffer);

    // Never drop delivered content: anchor at the oldest chunk when more
    // than the target is buffered (the correction loop slews the excess
    // latency down once the delivery cadence is known), or at
    // `edge - target` when the buffer still has to fill up. Starts forced
    // by the hold limit trim to the target instead, since that limit exists
    // to bound latency.
    let anchor_pts = match held_too_much {
        true => edge_pts.saturating_sub(target_buffer).min(max_pts),
        false => edge_pts.saturating_sub(target_buffer).min(min_pts),
    };
    let anchor = TimestampAnchor {
        input_pts: anchor_pts,
        output_pts,
    };
    info!(
        reason,
        ?edge,
        ?edge_distance,
        upper_edge = ?track_bounds.upper,
        lower_edge = ?track_bounds.lower,
        ?anchor,
        "Live sync track started"
    );
    Some(StartDecision {
        anchor,
        edge,
        edge_offset: elapsed.saturating_sub(edge_pts),
    })
}

/// Deviations within this distance from the expected buffer are left alone.
const CORRECTION_TOLERANCE: Duration = Duration::from_millis(500);
/// Largest single anchor adjustment; together with the check interval this
/// bounds the slew rate.
const MAX_CORRECTION_STEP: Duration = Duration::from_millis(10);
/// Deviation beyond this is an anomaly (pts discontinuity, long stall) that
/// slewing would chase for minutes; re-estimate the edge instead. A floor:
/// raised for batched delivery, whose sawtooth and keep-everything starts
/// legitimately deviate by multiples of the batch size.
const RESET_THRESHOLD: Duration = Duration::from_secs(10);

/// Post-start correction of a track's timestamp mapping.
#[derive(Debug, Clone, Copy)]
pub(super) enum AnchorCorrection {
    /// Buffer close enough to the expected size.
    None,
    /// Too much buffered; present content this much earlier.
    Earlier(Duration),
    /// Too little buffered; present content this much later.
    Later(Duration),
    /// Buffer diverged beyond correction; re-run the startup logic.
    Reset,
}

/// Checks how the track's delivery behaves relative to the playback position
/// and decides how to nudge the mapping back towards the desired buffer.
///
/// The estimated edge reacts to delivery changes only after its offset
/// window rotates; the newest delivered content reacts immediately, so it is
/// compared instead.
pub(super) fn decide_correction(
    options: &LiveSyncOptions,
    sync_point: Instant,
    now: Instant,
    estimator: &LiveEdgeEstimator,
    anchor: &TimestampAnchor,
) -> AnchorCorrection {
    let (Some(max_pts), Some(max_arrival_gap)) =
        (estimator.max_pts(), estimator.max_arrival_gap(now))
    else {
        return AnchorCorrection::None;
    };
    let elapsed = now.saturating_duration_since(sync_point);
    // newest delivered content relative to the playback position
    let delivered_buffer = anchor.to_output_pts(max_pts).saturating_sub(elapsed);

    let sustainable_buffer = max_arrival_gap * 3 / 2;
    let target_buffer = options.desired_buffer.max(sustainable_buffer);
    let expected = options.start_margin + target_buffer;
    // batched delivery makes the buffer saw between refills; the band has to
    // swallow the whole sawtooth
    let lower = expected.saturating_sub(sustainable_buffer + CORRECTION_TOLERANCE);
    let upper = expected + CORRECTION_TOLERANCE;
    let reset_threshold = RESET_THRESHOLD.max(sustainable_buffer * 2);

    if delivered_buffer > upper {
        let deviation = delivered_buffer - expected;
        match deviation > reset_threshold {
            true => AnchorCorrection::Reset,
            false => AnchorCorrection::Earlier(Duration::min(deviation / 8, MAX_CORRECTION_STEP)),
        }
    } else if delivered_buffer < lower {
        let deviation = expected - delivered_buffer;
        match deviation > reset_threshold {
            true => AnchorCorrection::Reset,
            false => AnchorCorrection::Later(Duration::min(deviation / 8, MAX_CORRECTION_STEP)),
        }
    } else {
        AnchorCorrection::None
    }
}
