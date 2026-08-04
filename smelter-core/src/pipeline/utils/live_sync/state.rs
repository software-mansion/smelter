use std::time::{Duration, Instant};

use super::{LiveSyncOptions, edge_estimator::LiveEdgeEstimator};

/// Correspondence between the input and output timelines chosen when a track
/// starts producing chunks: content at `input_pts` is presented at
/// `output_pts`, and every other timestamp keeps its distance to the anchor.
#[derive(Debug, Clone, Copy)]
pub(super) struct TimestampAnchor {
    /// Raw pts of the anchor: the pts presented right after the start, or the
    /// oldest buffered pts on flush.
    pub input_pts: Duration,
    /// Pts relative to the sync point at which content at `input_pts` is
    /// presented.
    pub output_pts: Duration,
}

impl TimestampAnchor {
    /// Maps a raw timestamp (pts or dts) onto the sync point timeline.
    /// Timestamps below `input_pts` (initial backlog) map before the start
    /// point, saturating at zero; such content plays late or is dropped by
    /// the consumer.
    pub(super) fn to_output_pts(&self, pts: Duration) -> Duration {
        (self.output_pts + pts).saturating_sub(self.input_pts)
    }

    // /// Shifts the mapping so content is presented `delta` earlier.
    // pub(super) fn shift_earlier(&mut self, delta: Duration) {
    //     self.input_pts += delta;
    // }

    // /// Shifts the mapping so content is presented `delta` later.
    // pub(super) fn shift_later(&mut self, delta: Duration) {
    //     self.output_pts += delta;
    // }
}

/// Cross-track mutable state of an input, kept behind a mutex.
pub(super) struct SharedState {
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    pub(super) shared_estimator: LiveEdgeEstimator,
}

/// Which live edge estimate a track aligned to when it started.
#[derive(Debug, Clone, Copy)]
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

// /// Deviations within this distance from the expected buffer are left alone.
// const CORRECTION_TOLERANCE: Duration = Duration::from_millis(500);
// /// Largest single anchor adjustment; together with the check interval this
// /// bounds the slew rate.
// const MAX_CORRECTION_STEP: Duration = Duration::from_millis(10);
// /// Deviation beyond this is an anomaly (pts discontinuity, long stall) that
// /// slewing would chase for minutes; re-estimate the edge instead. A floor:
// /// raised for batched delivery, whose sawtooth and keep-everything starts
// /// legitimately deviate by multiples of the batch size.
// const RESET_THRESHOLD: Duration = Duration::from_secs(10);
// 
// /// Post-start correction of a track's timestamp mapping.
// #[derive(Debug, Clone, Copy)]
// pub(super) enum AnchorCorrection {
//     /// Buffer close enough to the expected size.
//     None,
//     /// Too much buffered; present content this much earlier.
//     Earlier(Duration),
//     /// Too little buffered; present content this much later.
//     Later(Duration),
//     /// Buffer diverged beyond correction; re-run the startup logic.
//     Reset,
// }
// 
// /// Checks how the track's delivery behaves relative to the playback position
// /// and decides how to nudge the mapping back towards the desired buffer.
// ///
// /// The estimated edge reacts to delivery changes only after its offset
// /// window rotates; the newest delivered content reacts immediately, so it is
// /// compared instead.
// pub(super) fn decide_correction(
//     sync_point: Instant,
//     now: Instant,
//     opts: &LiveSyncOptions,
//     estimator: &LiveEdgeEstimator,
//     anchor: &TimestampAnchor,
// ) -> AnchorCorrection {
//     let estimation = estimator.estimate(now)?;
//     let last_pts = anchor.to_output_pts(estimation.delivery.last_pts);
//     let elapsed = now.saturating_duration_since(sync_point);
//     let effective_buffer = last_pts.saturating_sub(elapsed);
// 
//     // from elapsed + estimator I can resolve min,max anchor
//     // depending how current anchor falls into that range I can shift it
// 
//     let (Some(max_pts), Some(max_arrival_gap)) =
//         (estimator.max_pts(), estimator.max_arrival_gap(now))
//     else {
//         return AnchorCorrection::None;
//     };
//     let elapsed = now.saturating_duration_since(sync_point);
//     // newest delivered content relative to the playback position
//     let delivered_buffer = anchor.to_output_pts(max_pts).saturating_sub(elapsed);
// 
//     let sustainable_buffer = max_arrival_gap * 3 / 2;
//     let target_buffer = opts.desired_buffer.max(sustainable_buffer);
//     let expected = opts.start_margin + target_buffer;
//     // batched delivery makes the buffer saw between refills; the band has to
//     // swallow the whole sawtooth
//     let lower = expected.saturating_sub(sustainable_buffer + CORRECTION_TOLERANCE);
//     let upper = expected + CORRECTION_TOLERANCE;
//     let reset_threshold = RESET_THRESHOLD.max(sustainable_buffer * 2);
// 
//     if delivered_buffer > upper {
//         let deviation = delivered_buffer - expected;
//         match deviation > reset_threshold {
//             true => AnchorCorrection::Reset,
//             false => AnchorCorrection::Earlier(Duration::min(deviation / 8, MAX_CORRECTION_STEP)),
//         }
//     } else if delivered_buffer < lower {
//         let deviation = expected - delivered_buffer;
//         match deviation > reset_threshold {
//             true => AnchorCorrection::Reset,
//             false => AnchorCorrection::Later(Duration::min(deviation / 8, MAX_CORRECTION_STEP)),
//         }
//     } else {
//         AnchorCorrection::None
//     }
// }
