use std::time::{Duration, Instant};

use super::{buffer::LiveSyncBuffer, edge_estimator::EdgeEstimate, state::StartState};
use crate::{
    pipeline::utils::input_sync::{InputSyncStatsSender, TimestampAnchor, TrackKind},
    stats::{
        InputSyncMode, InputSyncStatsEvent, LiveSyncSnapshot, LiveSyncStatsEvent,
        LiveSyncTrackState,
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
    pub fn new(sender: InputSyncStatsSender, kind: TrackKind, sync_point: Instant) -> Self {
        sender.send(kind, InputSyncStatsEvent::TrackAdded(InputSyncMode::Live));
        Self {
            sender,
            kind,
            sync_point,
            last_snapshot: None,
        }
    }

    pub fn send_bytes_received(&self, size: usize) {
        self.sender
            .send(self.kind, InputSyncStatsEvent::BytesReceived(size));
    }

    pub fn send_state(&self, start: &StartState) {
        let state = match start {
            StartState::WaitingForStart => LiveSyncTrackState::WaitingForStart,
            StartState::StartedShared => LiveSyncTrackState::StartedShared,
            StartState::StartedTrack { .. } => LiveSyncTrackState::StartedTrack,
        };
        self.send(LiveSyncStatsEvent::StateChanged(state));
    }

    pub fn send_discontinuity(&self) {
        self.send(LiveSyncStatsEvent::Discontinuity);
    }

    /// `output_pts` is the pts of the chunk mapped onto the output timeline
    /// with the anchor the track applies.
    pub fn send_chunk_received(&self, output_pts: Duration, observed_at: Instant) {
        self.send(LiveSyncStatsEvent::ChunkReceived {
            effective_buffer_ns: self.effective_buffer_ns(output_pts, observed_at),
        });
    }

    pub fn send_chunk_output(&self, output_pts: Duration, observed_at: Instant) {
        self.send(LiveSyncStatsEvent::ChunkOutput {
            effective_buffer_ns: self.effective_buffer_ns(output_pts, observed_at),
        });
    }

    /// How much time content at `output_pts` has to reach the queue as of
    /// `observed_at`; negative when it is already late.
    fn effective_buffer_ns(&self, output_pts: Duration, observed_at: Instant) -> i64 {
        let now_pts = observed_at.saturating_duration_since(self.sync_point);
        output_pts.as_nanos() as i64 - now_pts.as_nanos() as i64
    }

    /// Throttled to [`SNAPSHOT_INTERVAL`]. `anchors` is `(current, target)`
    /// of the mapping the track applies and `estimate` the live edge estimate
    /// it is corrected against, both `None` before it started.
    pub fn send_snapshot(
        &mut self,
        now: Instant,
        buffer: &impl LiveSyncBuffer,
        anchors: Option<(TimestampAnchor, TimestampAnchor)>,
        estimate: Option<EdgeEstimate>,
    ) {
        if let Some(last) = self.last_snapshot
            && now.saturating_duration_since(last) < SNAPSHOT_INTERVAL
        {
            return;
        }
        self.last_snapshot = Some(now);
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
            Some(self.effective_buffer_ns(current.to_output_pts(bound_pts), now))
        };
        self.send(LiveSyncStatsEvent::Snapshot(LiveSyncSnapshot {
            buffer: buffer.stats(),
            target_offset_distance_ns,
            live_edge_lower_bound_distance_ns: estimate
                .and_then(|estimate| live_edge_distance_ns(estimate.lower_bound.pts)),
            live_edge_upper_bound_distance_ns: estimate
                .and_then(|estimate| live_edge_distance_ns(estimate.upper_bound.pts)),
        }));
    }

    fn send(&self, event: LiveSyncStatsEvent) {
        self.sender
            .send(self.kind, InputSyncStatsEvent::Live(event));
    }
}

impl Drop for LiveSyncTrackStats {
    fn drop(&mut self) {
        self.sender
            .send(self.kind, InputSyncStatsEvent::TrackRemoved);
    }
}
