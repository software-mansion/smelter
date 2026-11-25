use std::time::Duration;

use crate::{
    Timestamp,
    pipeline::utils::input_sync::TrackKind,
    stats::{
        input_reports::{
            FifoBufferStatsReport, InputSyncTrackStatsReport, JitterBufferStatsReport,
            LiveSyncBufferStatsReport, LiveSyncReanchorsStatsReport,
            LiveSyncTrackSlidingWindowStatsReport, LiveSyncTrackState, LiveSyncTrackStatsReport,
            SimpleSyncTrackState, SimpleSyncTrackStatsReport,
        },
        utils::SlidingWindowValue,
    },
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum InputSyncMode {
    Simple,
    Live,
}

/// Events sent by the input sync itself for a single track.
#[derive(Debug, Clone, Copy)]
pub(crate) enum InputSyncTrackStatsEvent {
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
    ChunkReleased {
        effective_buffer_ns: i64,
    },
    /// A re-anchor started.
    Reanchor {
        kind: LiveSyncReanchorKind,
        reason: LiveSyncReanchorReason,
    },
    /// Chunk left the sync buffer without being released.
    ChunkDropped,
    /// Video chunk was released to be shown after its time.
    LateChunkShown,
    /// Periodic snapshot of the sync state.
    StateSnapshot(LiveSyncTrackStateSnapshot),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LiveSyncReanchorKind {
    Jump,
    Slew,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LiveSyncReanchorReason {
    BufferTooSmall,
    BufferTooLarge,
    ContentLate,
    Interrupted,
    AddedLatencyTakenBack,
    TracksJoined,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncTrackStateSnapshot {
    /// Content held back by the sync.
    pub buffer: LiveSyncBufferStats,
    /// `None` before the track starts.
    pub playback: Option<LiveSyncPlaybackStats>,
    /// Total time the track had nothing to play since it started.
    pub interrupted: Duration,
}

/// Where playback of a track is, as distances behind the live edge of the stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncPlaybackStats {
    pub buffer: Timestamp,
    pub target_buffer: Timestamp,
    /// The range `buffer` does not start a re-anchor in.
    pub buffer_min: Timestamp,
    pub buffer_max: Timestamp,
    pub regular_lateness: Timestamp,
}

/// Snapshot of the content held in a sync buffer.
#[derive(Debug, Clone, Copy)]
pub(crate) enum LiveSyncBufferStats {
    Fifo {
        duration: Timestamp,
    },
    Jitter {
        duration: Timestamp,
        /// The next chunk is held back behind a gap that might still be filled.
        waiting_for_gap: bool,
    },
}

/// Per-input pair of track states, shared by every protocol using the input sync.
#[derive(Debug)]
pub struct InputSyncState {
    pub video: InputSyncTrackState,
    pub audio: InputSyncTrackState,
}

/// Stats of a single synchronized track.
#[derive(Debug)]
pub struct InputSyncTrackState {
    /// Stays set after the track is removed.
    was_added: bool,
    /// `None` while the track is not active.
    state: Option<InnerInputSyncTrackState>,
}

#[derive(Debug)]
struct InnerInputSyncTrackState {
    bitrate_1_sec: SlidingWindowValue<u64>,
    bitrate_1_min: SlidingWindowValue<u64>,
    mode: InputSyncModeState,
}

#[derive(Debug)]
enum InputSyncModeState {
    Simple { state: SimpleSyncTrackState },
    Live(Box<InputSyncLiveTrackState>),
}

#[derive(Debug)]
struct InputSyncLiveTrackState {
    state: LiveSyncTrackState,
    discontinuities_detected: u32,
    discontinuities_detected_10_secs: SlidingWindowValue<u32>,
    effective_buffer_on_receive_10_secs: SlidingWindowValue<i64>,
    effective_buffer_on_output_10_secs: SlidingWindowValue<i64>,
    reanchors: LiveSyncReanchorsStatsReport,
    chunks_dropped: u64,
    late_chunks_shown: u64,
    snapshot: LiveSyncTrackStateSnapshot,
}

impl InputSyncState {
    pub fn new() -> Self {
        Self {
            video: InputSyncTrackState::new(),
            audio: InputSyncTrackState::new(),
        }
    }

    pub fn handle_event(&mut self, track: TrackKind, event: InputSyncTrackStatsEvent) {
        match track {
            TrackKind::Video => self.video.handle_event(event),
            TrackKind::Audio => self.audio.handle_event(event),
        }
    }

    pub fn reset(&mut self) {
        self.video.state = None;
        self.audio.state = None;
    }
}

impl InputSyncTrackState {
    fn new() -> Self {
        Self {
            was_added: false,
            state: None,
        }
    }

    pub fn was_added(&self) -> bool {
        self.was_added
    }

    pub fn report(&mut self) -> Option<InputSyncTrackStatsReport> {
        let state = self.state.as_mut()?;
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
            InputSyncModeState::Live(live) => {
                let playback = live.snapshot.playback;
                InputSyncTrackStatsReport::Live(LiveSyncTrackStatsReport {
                    bitrate_1_second,
                    bitrate_1_minute,
                    state: live.state,
                    discontinuities_detected: live.discontinuities_detected,
                    buffer_seconds: playback.map(|playback| playback.buffer.as_secs_f64()),
                    target_buffer_seconds: playback
                        .map(|playback| playback.target_buffer.as_secs_f64()),
                    buffer_min_seconds: playback.map(|playback| playback.buffer_min.as_secs_f64()),
                    buffer_max_seconds: playback.map(|playback| playback.buffer_max.as_secs_f64()),
                    regular_lateness_seconds: playback
                        .map(|playback| playback.regular_lateness.as_secs_f64()),
                    reanchors: live.reanchors,
                    chunks_dropped: live.chunks_dropped,
                    late_chunks_shown: live.late_chunks_shown,
                    interrupted_seconds: live.snapshot.interrupted.as_secs_f64(),
                    buffer: match live.snapshot.buffer {
                        LiveSyncBufferStats::Fifo { duration } => {
                            LiveSyncBufferStatsReport::Fifo(FifoBufferStatsReport {
                                duration_seconds: duration.as_secs_f64(),
                            })
                        }
                        LiveSyncBufferStats::Jitter {
                            duration,
                            waiting_for_gap,
                        } => LiveSyncBufferStatsReport::Jitter(JitterBufferStatsReport {
                            duration_seconds: duration.as_secs_f64(),
                            waiting_for_gap,
                        }),
                    },
                    last_10_seconds: LiveSyncTrackSlidingWindowStatsReport {
                        discontinuities_detected: live.discontinuities_detected_10_secs.sum(),
                        effective_buffer_on_receive_avg_seconds: avg_secs(
                            &mut live.effective_buffer_on_receive_10_secs,
                        ),
                        effective_buffer_on_receive_max_seconds: ns_to_secs(
                            live.effective_buffer_on_receive_10_secs.max(),
                        ),
                        effective_buffer_on_receive_min_seconds: ns_to_secs(
                            live.effective_buffer_on_receive_10_secs.min(),
                        ),
                        effective_buffer_on_output_avg_seconds: avg_secs(
                            &mut live.effective_buffer_on_output_10_secs,
                        ),
                        effective_buffer_on_output_max_seconds: ns_to_secs(
                            live.effective_buffer_on_output_10_secs.max(),
                        ),
                        effective_buffer_on_output_min_seconds: ns_to_secs(
                            live.effective_buffer_on_output_10_secs.min(),
                        ),
                    },
                })
            }
        };
        Some(report)
    }

    fn handle_event(&mut self, event: InputSyncTrackStatsEvent) {
        match event {
            InputSyncTrackStatsEvent::TrackAdded(mode) => {
                self.was_added = true;
                self.state = Some(InnerInputSyncTrackState::new(mode));
            }
            InputSyncTrackStatsEvent::TrackRemoved => {
                self.state = None;
            }
            InputSyncTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let Some(state) = self.state.as_mut() else {
                    return;
                };
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                state.bitrate_1_sec.push(chunk_size_bits);
                state.bitrate_1_min.push(chunk_size_bits);
            }
            InputSyncTrackStatsEvent::Simple(event) => {
                let Some(state) = self.state.as_mut() else {
                    return;
                };
                let InputSyncModeState::Simple { state } = &mut state.mode else {
                    tracing::error!(?event, "Simple sync event on live sync track");
                    return;
                };
                match event {
                    SimpleSyncStatsEvent::StateChanged(new_state) => *state = new_state,
                }
            }
            InputSyncTrackStatsEvent::Live(event) => {
                let Some(state) = self.state.as_mut() else {
                    return;
                };
                let InputSyncModeState::Live(live) = &mut state.mode else {
                    tracing::error!(?event, "Live sync event on simple sync track");
                    return;
                };
                match event {
                    LiveSyncStatsEvent::StateChanged(new_state) => live.state = new_state,
                    LiveSyncStatsEvent::Discontinuity => {
                        live.discontinuities_detected += 1;
                        live.discontinuities_detected_10_secs.push(1);
                    }
                    LiveSyncStatsEvent::ChunkReceived {
                        effective_buffer_ns,
                    } => live
                        .effective_buffer_on_receive_10_secs
                        .push(effective_buffer_ns),
                    LiveSyncStatsEvent::ChunkReleased {
                        effective_buffer_ns,
                    } => live
                        .effective_buffer_on_output_10_secs
                        .push(effective_buffer_ns),
                    LiveSyncStatsEvent::Reanchor { kind, reason } => {
                        let reanchors = &mut live.reanchors;
                        match kind {
                            LiveSyncReanchorKind::Jump => reanchors.jumps += 1,
                            LiveSyncReanchorKind::Slew => reanchors.slews += 1,
                        }
                        match reason {
                            LiveSyncReanchorReason::BufferTooSmall => {
                                reanchors.buffer_too_small += 1
                            }
                            LiveSyncReanchorReason::BufferTooLarge => {
                                reanchors.buffer_too_large += 1
                            }
                            LiveSyncReanchorReason::ContentLate => reanchors.content_late += 1,
                            LiveSyncReanchorReason::Interrupted => reanchors.interrupted += 1,
                            LiveSyncReanchorReason::AddedLatencyTakenBack => {
                                reanchors.added_latency_taken_back += 1
                            }
                            LiveSyncReanchorReason::TracksJoined => reanchors.tracks_joined += 1,
                        }
                    }
                    LiveSyncStatsEvent::ChunkDropped => live.chunks_dropped += 1,
                    LiveSyncStatsEvent::LateChunkShown => live.late_chunks_shown += 1,
                    LiveSyncStatsEvent::StateSnapshot(new_snapshot) => live.snapshot = new_snapshot,
                }
            }
        }
    }
}

impl InnerInputSyncTrackState {
    fn new(mode: InputSyncMode) -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
            mode: match mode {
                InputSyncMode::Simple => InputSyncModeState::Simple {
                    state: SimpleSyncTrackState::InitialBuffering,
                },
                InputSyncMode::Live => {
                    InputSyncModeState::Live(Box::new(InputSyncLiveTrackState {
                        state: LiveSyncTrackState::WaitingForStart,
                        discontinuities_detected: 0,
                        discontinuities_detected_10_secs: SlidingWindowValue::new(
                            Duration::from_secs(10),
                        ),
                        effective_buffer_on_receive_10_secs: SlidingWindowValue::new(
                            Duration::from_secs(10),
                        ),
                        effective_buffer_on_output_10_secs: SlidingWindowValue::new(
                            Duration::from_secs(10),
                        ),
                        reanchors: LiveSyncReanchorsStatsReport::default(),
                        chunks_dropped: 0,
                        late_chunks_shown: 0,
                        snapshot: LiveSyncTrackStateSnapshot {
                            buffer: LiveSyncBufferStats::Fifo {
                                duration: Timestamp::ZERO,
                            },
                            playback: None,
                            interrupted: Duration::ZERO,
                        },
                    }))
                }
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
