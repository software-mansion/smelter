use std::time::Duration;

use crate::{
    pipeline::utils::input_sync::TrackKind,
    stats::{
        input_reports::{
            FifoBufferStatsReport, InputSyncTrackStatsReport, LiveSyncBufferStatsReport,
            LiveSyncTrackSlidingWindowStatsReport, LiveSyncTrackState, LiveSyncTrackStatsReport,
            SimpleSyncTrackState, SimpleSyncTrackStatsReport,
        },
        utils::SlidingWindowValue,
    },
};

/// Events sent by the input sync itself for a single track.
#[derive(Debug, Clone, Copy)]
pub(crate) enum InputSyncStatsEvent {
    /// Track registered on the sync of the given mode.
    TrackAdded(InputSyncMode),
    /// Track dropped; its stats are no longer reported.
    TrackRemoved,
    /// Chunk written to the track.
    BytesReceived(usize),
    Simple(SimpleSyncStatsEvent),
    Live(LiveSyncStatsEvent),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum InputSyncMode {
    Simple,
    Live,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SimpleSyncStatsEvent {
    StateChanged(SimpleSyncTrackState),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LiveSyncStatsEvent {
    StateChanged(LiveSyncTrackState),
    Discontinuity,
    /// Chunk entered the sync buffer of a started track; `effective_buffer_ns`
    /// is how much time it has to reach the queue with the current mapping,
    /// negative when it is already late.
    ChunkReceived {
        effective_buffer_ns: i64,
    },
    /// Chunk left the sync buffer; `effective_buffer_ns` is how much time it
    /// has to reach the queue, negative when it is already late.
    ChunkOutput {
        effective_buffer_ns: i64,
    },
    /// Periodic snapshot of the sync state.
    Snapshot(LiveSyncSnapshot),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncSnapshot {
    pub buffer: LiveSyncBufferStats,
    /// Signed distance between the current and the target anchor; positive
    /// when the buffer is being shrunk.
    pub target_offset_distance_ns: i64,
    /// How far the playback position is behind the live edge estimate
    /// bounds; `None` before the track starts.
    pub live_edge_lower_bound_distance_ns: Option<i64>,
    pub live_edge_upper_bound_distance_ns: Option<i64>,
}

/// Snapshot of the content held in a sync buffer.
#[derive(Debug, Clone, Copy)]
pub(crate) enum LiveSyncBufferStats {
    Fifo { duration: Duration },
}

/// Stats of a single synchronized track; `None` until the track is added.
#[derive(Debug)]
pub struct InputSyncTrackState(Option<InputSyncTrackModeState>);

#[derive(Debug)]
struct InputSyncTrackModeState {
    bitrate_1_sec: SlidingWindowValue<u64>,
    bitrate_1_min: SlidingWindowValue<u64>,
    mode: InputSyncModeState,
}

#[derive(Debug)]
enum InputSyncModeState {
    Simple {
        state: SimpleSyncTrackState,
    },
    Live {
        state: LiveSyncTrackState,
        discontinuities_detected: u32,
        discontinuities_detected_10_secs: SlidingWindowValue<u32>,
        effective_buffer_on_receive_10_secs: SlidingWindowValue<i64>,
        effective_buffer_on_output_10_secs: SlidingWindowValue<i64>,
        snapshot: LiveSyncSnapshot,
    },
}

/// Per-input pair of track states, shared by every protocol using the input sync.
#[derive(Debug)]
pub struct InputSyncState {
    pub video: InputSyncTrackState,
    pub audio: InputSyncTrackState,
}

impl InputSyncState {
    pub fn new() -> Self {
        Self {
            video: InputSyncTrackState(None),
            audio: InputSyncTrackState(None),
        }
    }

    pub fn handle_event(&mut self, track: TrackKind, event: InputSyncStatsEvent) {
        match track {
            TrackKind::Video => self.video.handle_event(event),
            TrackKind::Audio => self.audio.handle_event(event),
        }
    }

    pub fn reset(&mut self) {
        self.video.0 = None;
        self.audio.0 = None;
    }
}

impl InputSyncTrackState {
    pub fn report(&mut self) -> Option<InputSyncTrackStatsReport> {
        let state = self.0.as_mut()?;
        let bitrate_1_second =
            state.bitrate_1_sec.sum() / state.bitrate_1_sec.window_size().as_secs();
        let bitrate_1_minute =
            state.bitrate_1_min.sum() / state.bitrate_1_min.window_size().as_secs();
        let report = match &mut state.mode {
            InputSyncModeState::Simple { state } => {
                InputSyncTrackStatsReport::Simple(SimpleSyncTrackStatsReport {
                    bitrate_1_second,
                    bitrate_1_minute,
                    state: *state,
                })
            }
            InputSyncModeState::Live {
                state,
                discontinuities_detected,
                discontinuities_detected_10_secs,
                effective_buffer_on_receive_10_secs,
                effective_buffer_on_output_10_secs,
                snapshot,
            } => InputSyncTrackStatsReport::Live(LiveSyncTrackStatsReport {
                bitrate_1_second,
                bitrate_1_minute,
                state: *state,
                discontinuities_detected: *discontinuities_detected,
                target_offset_distance_seconds: ns_to_secs(snapshot.target_offset_distance_ns),
                live_edge_lower_bound_distance_seconds: snapshot
                    .live_edge_lower_bound_distance_ns
                    .map(ns_to_secs),
                live_edge_upper_bound_distance_seconds: snapshot
                    .live_edge_upper_bound_distance_ns
                    .map(ns_to_secs),
                buffer: match snapshot.buffer {
                    LiveSyncBufferStats::Fifo { duration } => {
                        LiveSyncBufferStatsReport::Fifo(FifoBufferStatsReport {
                            duration_seconds: duration.as_secs_f64(),
                        })
                    }
                },
                last_10_seconds: LiveSyncTrackSlidingWindowStatsReport {
                    discontinuities_detected: discontinuities_detected_10_secs.sum(),
                    effective_buffer_on_receive_avg_seconds: avg_secs(
                        effective_buffer_on_receive_10_secs,
                    ),
                    effective_buffer_on_receive_max_seconds: ns_to_secs(
                        effective_buffer_on_receive_10_secs.max(),
                    ),
                    effective_buffer_on_receive_min_seconds: ns_to_secs(
                        effective_buffer_on_receive_10_secs.min(),
                    ),
                    effective_buffer_on_output_avg_seconds: avg_secs(
                        effective_buffer_on_output_10_secs,
                    ),
                    effective_buffer_on_output_max_seconds: ns_to_secs(
                        effective_buffer_on_output_10_secs.max(),
                    ),
                    effective_buffer_on_output_min_seconds: ns_to_secs(
                        effective_buffer_on_output_10_secs.min(),
                    ),
                },
            }),
        };
        Some(report)
    }

    fn handle_event(&mut self, event: InputSyncStatsEvent) {
        match event {
            InputSyncStatsEvent::TrackAdded(mode) => {
                self.0 = Some(InputSyncTrackModeState::new(mode));
                return;
            }
            InputSyncStatsEvent::TrackRemoved => {
                self.0 = None;
                return;
            }
            _ => {}
        }
        let Some(state) = self.0.as_mut() else {
            return;
        };
        match (event, &mut state.mode) {
            (InputSyncStatsEvent::TrackAdded(_) | InputSyncStatsEvent::TrackRemoved, _) => {
                unreachable!()
            }
            (InputSyncStatsEvent::BytesReceived(chunk_size_bytes), _) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                state.bitrate_1_sec.push(chunk_size_bits);
                state.bitrate_1_min.push(chunk_size_bits);
            }
            (
                InputSyncStatsEvent::Simple(SimpleSyncStatsEvent::StateChanged(new_state)),
                InputSyncModeState::Simple { state },
            ) => *state = new_state,
            (
                InputSyncStatsEvent::Live(event),
                InputSyncModeState::Live {
                    state,
                    discontinuities_detected,
                    discontinuities_detected_10_secs,
                    effective_buffer_on_receive_10_secs,
                    effective_buffer_on_output_10_secs,
                    snapshot,
                },
            ) => match event {
                LiveSyncStatsEvent::StateChanged(new_state) => *state = new_state,
                LiveSyncStatsEvent::Discontinuity => {
                    *discontinuities_detected += 1;
                    discontinuities_detected_10_secs.push(1);
                }
                LiveSyncStatsEvent::ChunkReceived {
                    effective_buffer_ns,
                } => effective_buffer_on_receive_10_secs.push(effective_buffer_ns),
                LiveSyncStatsEvent::ChunkOutput {
                    effective_buffer_ns,
                } => effective_buffer_on_output_10_secs.push(effective_buffer_ns),
                LiveSyncStatsEvent::Snapshot(new_snapshot) => *snapshot = new_snapshot,
            },
            (event, mode) => tracing::error!(?event, ?mode, "Wrong event type for sync mode"),
        }
    }
}

impl InputSyncTrackModeState {
    fn new(mode: InputSyncMode) -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
            mode: match mode {
                InputSyncMode::Simple => InputSyncModeState::Simple {
                    state: SimpleSyncTrackState::InitialBuffering,
                },
                InputSyncMode::Live => InputSyncModeState::Live {
                    state: LiveSyncTrackState::WaitingForStart,
                    discontinuities_detected: 0,
                    discontinuities_detected_10_secs: SlidingWindowValue::new(Duration::from_secs(
                        10,
                    )),
                    effective_buffer_on_receive_10_secs: SlidingWindowValue::new(
                        Duration::from_secs(10),
                    ),
                    effective_buffer_on_output_10_secs: SlidingWindowValue::new(
                        Duration::from_secs(10),
                    ),
                    snapshot: LiveSyncSnapshot {
                        buffer: LiveSyncBufferStats::Fifo {
                            duration: Duration::ZERO,
                        },
                        target_offset_distance_ns: 0,
                        live_edge_lower_bound_distance_ns: None,
                        live_edge_upper_bound_distance_ns: None,
                    },
                },
            },
        }
    }
}

fn ns_to_secs(ns: i64) -> f64 {
    ns as f64 / 1_000_000_000.0
}

fn avg_secs(window: &mut SlidingWindowValue<i64>) -> f64 {
    ns_to_secs(window.sum() / i64::max(window.count() as i64, 1))
}
