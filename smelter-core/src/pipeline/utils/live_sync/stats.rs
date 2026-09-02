use std::time::{Duration, Instant};

use super::{buffer::LiveSyncBuffer, edge_estimator::LiveEdgeEstimator, state::StartState};
use crate::{
    pipeline::utils::input_sync::{InputSyncStatsSender, TimestampAnchor, TrackKind},
    stats::{
        InputSyncMode, InputSyncTrackStatsEvent, LiveSyncStatsEvent, LiveSyncTrackState,
        LiveSyncTrackStateSnapshot,
    },
};

/// How often snapshots are reported.
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(200);

/// Reports the state of one live sync track to the stats module.
pub(super) struct LiveSyncTrackStats {
    sender: InputSyncStatsSender,
    kind: TrackKind,
    sync_point: Instant,
    last_snapshot: Option<Instant>,
}

impl LiveSyncTrackStats {
    pub fn new(sender: &InputSyncStatsSender, kind: TrackKind, sync_point: Instant) -> Self {
        sender.send(
            kind,
            InputSyncTrackStatsEvent::TrackAdded(InputSyncMode::Live),
        );
        Self {
            sender: sender.clone(),
            kind,
            sync_point,
            last_snapshot: None,
        }
    }

    pub fn report_bytes_received(&self, size: usize) {
        self.sender
            .send(self.kind, InputSyncTrackStatsEvent::BytesReceived(size));
    }

    pub fn report_state_change(&self, start: &StartState) {
        let state = match start {
            StartState::WaitingForStart => LiveSyncTrackState::WaitingForStart,
            StartState::StartedShared => LiveSyncTrackState::StartedShared,
            StartState::StartedTrack { .. } => LiveSyncTrackState::StartedTrack,
        };
        self.send(LiveSyncStatsEvent::StateChanged(state));
    }

    pub fn report_discontinuity(&self) {
        self.send(LiveSyncStatsEvent::Discontinuity);
    }

    pub fn report_chunk_received(&self, output_pts: Duration) {
        self.send(LiveSyncStatsEvent::ChunkReceived {
            effective_buffer_ns: self.effective_buffer_ns(output_pts),
        });
    }

    pub fn report_chunk_released(&self, output_pts: Duration) {
        self.send(LiveSyncStatsEvent::ChunkReleased {
            effective_buffer_ns: self.effective_buffer_ns(output_pts),
        });
    }

    /// How much time content at `output_pts` has to reach the queue as of
    /// `observed_at`; negative when it is already late.
    fn effective_buffer_ns(&self, output_pts: Duration) -> i64 {
        output_pts.as_nanos() as i64 - self.sync_point.elapsed().as_nanos() as i64
    }

    /// Throttled to [`SNAPSHOT_INTERVAL`]. `anchors` is `(current, target)`
    /// of the mapping the track applies and `estimator` the live edge
    /// estimator it is corrected against, both `None` before it started.
    pub fn report_state_snapshot(
        &mut self,
        buffer: &impl LiveSyncBuffer,
        anchors: Option<(TimestampAnchor, TimestampAnchor)>,
        estimator: Option<&LiveEdgeEstimator>,
    ) {
        let now = Instant::now();
        if let Some(last) = self.last_snapshot
            && now.saturating_duration_since(last) < SNAPSHOT_INTERVAL
        {
            return;
        }
        self.last_snapshot = Some(now);
        let estimate = estimator.and_then(|estimator| estimator.estimate(now));
        let target_offset_distance_ns = match anchors {
            Some((current, target)) => {
                let distance = current.distance_to(target).as_nanos() as i64;
                match current.presents_later_than(target) {
                    true => distance,
                    false => -distance,
                }
            }
            None => 0,
        };
        let live_edge_distance_ns = |bound_pts: Duration| {
            let (current, _) = anchors?;
            Some(self.effective_buffer_ns(current.to_output_pts(bound_pts)))
        };
        self.send(LiveSyncStatsEvent::StateSnapshot(
            LiveSyncTrackStateSnapshot {
                buffer: buffer.stats(),
                target_offset_distance_ns,
                live_edge_lower_bound_distance_ns: estimate
                    .and_then(|estimate| live_edge_distance_ns(estimate.lower_bound.pts)),
                live_edge_upper_bound_distance_ns: estimate
                    .and_then(|estimate| live_edge_distance_ns(estimate.upper_bound.pts)),
            },
        ));
    }

    fn send(&self, event: LiveSyncStatsEvent) {
        self.sender
            .send(self.kind, InputSyncTrackStatsEvent::Live(event));
    }
}

impl Drop for LiveSyncTrackStats {
    fn drop(&mut self) {
        self.sender
            .send(self.kind, InputSyncTrackStatsEvent::TrackRemoved);
    }
}
