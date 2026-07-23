use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use tracing::{debug, info};

use super::{LiveSyncOptions, edge_estimator::LiveEdgeEstimator};

/// Correspondence between the input and output timelines chosen when the sync
/// starts producing chunks: content at `input_pts` is presented at
/// `output_pts`, and every other timestamp keeps its distance to the anchor.
#[derive(Debug, Clone, Copy)]
pub(super) struct TimestampAnchor {
    /// Raw pts of the anchor: the cut point, or the oldest buffered pts when
    /// nothing was trimmed.
    input_pts: Duration,
    /// Pts relative to the sync point at which content at `input_pts` is
    /// presented.
    output_pts: Duration,
}

impl TimestampAnchor {
    /// Maps a raw timestamp (pts or dts) onto the sync point timeline.
    /// Saturating, but effectively always positive: timestamps below
    /// `input_pts` (e.g. leading B-frames after a trimmed keyframe) stay
    /// above zero thanks to `start_margin` included in `output_pts`.
    pub(super) fn to_output_pts(&self, pts: Duration) -> Duration {
        (self.output_pts + pts).saturating_sub(self.input_pts)
    }
}

pub(super) struct SharedState {
    pub(super) options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    pub(super) sync_point: Instant,
    pub(super) detection: Mutex<StartDetection>,
}

pub(super) struct StartDetection {
    pub(super) tracks: Vec<TrackMeta>,
    pub(super) estimator: LiveEdgeEstimator,
    /// Buffer was too small when the live edge stabilized; waiting until it
    /// grows to the desired size.
    pub(super) filling: bool,
    pub(super) start: Option<StartDecision>,
}

#[derive(Default)]
pub(super) struct TrackMeta {
    /// Track contains items that are not start points (e.g. video with
    /// interframes); such tracks can only be cut at a start point.
    constrained: bool,
    /// pts of start point items observed while buffering.
    start_points: Vec<Duration>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StartDecision {
    pub(super) anchor: TimestampAnchor,
    /// Trim each track's buffer to start near this pts; `None` keeps
    /// everything.
    pub(super) cut_pts: Option<Duration>,
}

impl StartDetection {
    pub(super) fn observe(
        &mut self,
        track_index: usize,
        now: Instant,
        pts: Duration,
        is_start_point: bool,
    ) {
        let track = &mut self.tracks[track_index];
        match is_start_point {
            true => track.start_points.push(pts),
            false => track.constrained = true,
        }
        self.estimator.observe(now, pts);
    }

    pub(super) fn maybe_start(&mut self, shared: &SharedState, now: Instant) {
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

    pub(super) fn start_now(
        &mut self,
        shared: &SharedState,
        now: Instant,
        cutoff: Option<Duration>,
        reason: &str,
    ) {
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
        let anchor = TimestampAnchor {
            input_pts: anchor_pts,
            output_pts: now.saturating_duration_since(shared.sync_point)
                + shared.options.start_margin,
        };
        info!(reason, ?cut_pts, ?anchor, "Live sync started");
        self.start = Some(StartDecision { anchor, cut_pts });
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
