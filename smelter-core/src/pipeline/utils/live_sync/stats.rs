use std::time::Duration;

use super::{
    buffer::LiveSyncBuffer,
    decide::{Playback, Reanchor, ReanchorKind, ReanchorReason},
};
use crate::{
    Timestamp,
    pipeline::utils::input_sync::{InputSyncStatsSender, TrackKind},
    stats::{
        InputSyncMode, InputSyncTrackStatsEvent, LiveSyncPlaybackStats, LiveSyncReanchorKind,
        LiveSyncReanchorReason, LiveSyncStatsEvent, LiveSyncTrackState, LiveSyncTrackStateSnapshot,
    },
};

/// How often snapshots are reported.
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(200);

/// Reports the state of one live sync track to the stats module.
pub(super) struct LiveSyncTrackStats {
    sender: InputSyncStatsSender,
    kind: TrackKind,
    last_snapshot: Option<Timestamp>,
    /// The state on the last report, and when it was.
    last_state: Option<(LiveSyncTrackState, Timestamp)>,
    /// Total time in [`LiveSyncTrackState::Interrupted`].
    interrupted: Duration,
}

impl LiveSyncTrackStats {
    pub fn new(sender: &InputSyncStatsSender, kind: TrackKind) -> Self {
        sender.send(
            kind,
            InputSyncTrackStatsEvent::TrackAdded(InputSyncMode::Live),
        );
        Self {
            sender: sender.clone(),
            kind,
            last_snapshot: None,
            last_state: None,
            interrupted: Duration::ZERO,
        }
    }

    pub fn report_bytes_received(&self, size: usize) {
        self.sender
            .send(self.kind, InputSyncTrackStatsEvent::BytesReceived(size));
    }

    pub fn report_discontinuity(&self) {
        self.send(LiveSyncStatsEvent::Discontinuity);
    }

    /// `effective_buffer`: how much time the chunk has to reach the queue; negative when it is
    /// already late.
    pub fn report_chunk_received(&self, effective_buffer: Timestamp) {
        self.send(LiveSyncStatsEvent::ChunkReceived {
            effective_buffer_ns: effective_buffer.as_nanos(),
        });
    }

    pub fn report_chunk_released(&self, effective_buffer: Timestamp) {
        self.send(LiveSyncStatsEvent::ChunkReleased {
            effective_buffer_ns: effective_buffer.as_nanos(),
        });
    }

    pub fn report_chunk_dropped(&self) {
        self.send(LiveSyncStatsEvent::ChunkDropped);
    }

    pub fn report_late_chunk_shown(&self) {
        self.send(LiveSyncStatsEvent::LateChunkShown);
    }

    /// A re-anchor of the timeline the track is on started.
    pub fn report_reanchor(&self, reanchor: Reanchor) {
        self.send(LiveSyncStatsEvent::Reanchor {
            kind: match reanchor.kind {
                ReanchorKind::Jump => LiveSyncReanchorKind::Jump,
                ReanchorKind::Slew => LiveSyncReanchorKind::Slew,
            },
            reason: match reanchor.reason {
                ReanchorReason::BufferTooSmall => LiveSyncReanchorReason::BufferTooSmall,
                ReanchorReason::BufferTooLarge => LiveSyncReanchorReason::BufferTooLarge,
                ReanchorReason::ContentLate => LiveSyncReanchorReason::ContentLate,
                ReanchorReason::Interrupted => LiveSyncReanchorReason::Interrupted,
                ReanchorReason::AddedLatencyTakenBack => {
                    LiveSyncReanchorReason::AddedLatencyTakenBack
                }
                ReanchorReason::TracksJoined => LiveSyncReanchorReason::TracksJoined,
            },
        });
    }

    /// Called after every run of the stages. The state is reported when it changes, the snapshot
    /// once per [`SNAPSHOT_INTERVAL`]. `playback`: the timeline the track is scheduled on.
    pub fn report_state(
        &mut self,
        now: Timestamp,
        state: LiveSyncTrackState,
        buffer: &impl LiveSyncBuffer,
        playback: Option<&Playback>,
    ) {
        let last_state = self.last_state.replace((state, now));
        if let Some((LiveSyncTrackState::Interrupted, since)) = last_state {
            self.interrupted += (now - since).to_duration_saturating();
        }
        if last_state.is_none_or(|(last_state, _)| last_state != state) {
            self.send(LiveSyncStatsEvent::StateChanged(state));
        }

        if let Some(last) = self.last_snapshot
            && now < last + SNAPSHOT_INTERVAL
        {
            return;
        }
        self.last_snapshot = Some(now);
        let playback = playback.and_then(|playback| {
            let status = playback.status?;
            Some(LiveSyncPlaybackStats {
                buffer: playback.timeline.value() - status.edge,
                target_buffer: status.target - status.edge,
                buffer_min: status.lowest - status.edge,
                buffer_max: status.highest - status.edge,
                regular_lateness: status.lateness,
            })
        });
        self.send(LiveSyncStatsEvent::StateSnapshot(
            LiveSyncTrackStateSnapshot {
                buffer: buffer.stats(),
                playback,
                interrupted: self.interrupted,
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
