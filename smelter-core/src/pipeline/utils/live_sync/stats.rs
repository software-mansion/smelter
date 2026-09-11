use std::time::{Duration, Instant};

use super::{
    buffer::LiveSyncBuffer,
    edge_estimator::LiveEdgeEstimator,
    mode::{Mode, TrackState},
};
use crate::{
    InstantExt, Timestamp,
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
    last_state: Option<LiveSyncTrackState>,
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
            last_state: None,
        }
    }

    pub fn report_bytes_received(&self, size: usize) {
        self.sender
            .send(self.kind, InputSyncTrackStatsEvent::BytesReceived(size));
    }

    /// Reports the state a track is in only when it differs from the last
    /// report, so it can be called after every tick.
    pub fn report_state_change(&mut self, track: TrackState, mode: Mode) {
        let state = match (track, mode) {
            (TrackState::Waiting, _) => LiveSyncTrackState::WaitingForStart,
            (TrackState::Started, Mode::Shared(_)) => LiveSyncTrackState::StartedShared,
            (TrackState::Started, Mode::Independent { .. }) => LiveSyncTrackState::StartedTrack,
            // a started track always has an anchor, so the mode is decided
            (TrackState::Started, Mode::Undecided) => LiveSyncTrackState::WaitingForStart,
        };
        if self.last_state == Some(state) {
            return;
        }
        self.last_state = Some(state);
        self.send(LiveSyncStatsEvent::StateChanged(state));
    }

    pub fn report_discontinuity(&self) {
        self.send(LiveSyncStatsEvent::Discontinuity);
    }

    pub fn report_chunk_received(&self, output_pts: Timestamp) {
        self.send(LiveSyncStatsEvent::ChunkReceived {
            effective_buffer_ns: self.effective_buffer(output_pts).as_nanos(),
        });
    }

    pub fn report_chunk_released(&self, output_pts: Timestamp) {
        self.send(LiveSyncStatsEvent::ChunkReleased {
            effective_buffer_ns: self.effective_buffer(output_pts).as_nanos(),
        });
    }

    /// How much time content at `output_pts` has to reach the queue as of now; negative when it
    /// is already late.
    fn effective_buffer(&self, output_pts: Timestamp) -> Timestamp {
        output_pts - self.sync_point.timestamp_now()
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
        let target_offset_distance = match anchors {
            Some((current, target)) => current.as_offset() - target.as_offset(),
            None => Timestamp::ZERO,
        };
        let live_edge_distance = |bound_pts: Timestamp| {
            let (current, _) = anchors?;
            Some(self.effective_buffer(current.to_output_pts(bound_pts)))
        };
        self.send(LiveSyncStatsEvent::StateSnapshot(
            LiveSyncTrackStateSnapshot {
                buffer: buffer.stats(),
                target_offset_distance,
                live_edge_lower_bound_distance: estimate
                    .and_then(|estimate| live_edge_distance(estimate.lower_bound.pts)),
                live_edge_upper_bound_distance: estimate
                    .and_then(|estimate| live_edge_distance(estimate.upper_bound.pts)),
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
