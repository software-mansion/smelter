use std::time::Duration;

use smelter_render::OutputId;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{WhipOutputStatsReport, WhipOutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhipOutputStatsEvent {
    Video(WhipOutputTrackStatsEvent),
    Audio(WhipOutputTrackStatsEvent),
    PeerStateChanged(RTCPeerConnectionState),
}

impl WhipOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Whip(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhipOutputTrackStatsEvent {
    BytesSent(usize),
}

impl WhipOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => WhipOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => WhipOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct WhipOutputState {
    pub video: WhipOutputTrackState,
    pub audio: WhipOutputTrackState,
    pub peer_state: RTCPeerConnectionState,
}

#[derive(Debug)]
pub struct WhipOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl WhipOutputState {
    pub fn new() -> Self {
        Self {
            video: WhipOutputTrackState::new(),
            audio: WhipOutputTrackState::new(),
            peer_state: RTCPeerConnectionState::New,
        }
    }

    pub fn report(&mut self) -> WhipOutputStatsReport {
        WhipOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
            is_connected: self.peer_state == RTCPeerConnectionState::Connected,
        }
    }

    pub fn handle_event(&mut self, event: WhipOutputStatsEvent) {
        match event {
            WhipOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            WhipOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
            WhipOutputStatsEvent::PeerStateChanged(state) => self.peer_state = state,
        }
    }
}

impl WhipOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> WhipOutputTrackStatsReport {
        WhipOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_avg_1_minute: self.bitrate_1_min.sum()
                / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: WhipOutputTrackStatsEvent) {
        match event {
            WhipOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
