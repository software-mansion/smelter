use std::{collections::HashMap, sync::Arc, time::Duration};

use smelter_render::OutputId;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{WhepOutputStatsReport, WhepOutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone)]
pub(crate) enum WhepOutputStatsEvent {
    Video(WhepOutputTrackStatsEvent),
    Audio(WhepOutputTrackStatsEvent),
    PeerStateChanged {
        session_id: Arc<str>,
        state: RTCPeerConnectionState,
    },
}

impl WhepOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Whep(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepOutputTrackStatsEvent {
    BytesSent(usize),
}

impl WhepOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => WhepOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => WhepOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct WhepOutputState {
    pub video: WhepOutputTrackState,
    pub audio: WhepOutputTrackState,
    pub peers: HashMap<Arc<str>, RTCPeerConnectionState>,
}

#[derive(Debug)]
pub struct WhepOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl WhepOutputState {
    pub fn new() -> Self {
        Self {
            video: WhepOutputTrackState::new(),
            audio: WhepOutputTrackState::new(),
            peers: HashMap::new(),
        }
    }

    pub fn report(&mut self) -> WhepOutputStatsReport {
        let connected_peers = self
            .peers
            .values()
            .filter(|state| **state == RTCPeerConnectionState::Connected)
            .count() as u64;

        WhepOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
            connected_peers,
        }
    }

    pub fn handle_event(&mut self, event: WhepOutputStatsEvent) {
        match event {
            WhepOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            WhepOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
            WhepOutputStatsEvent::PeerStateChanged { session_id, state } => {
                self.peers.insert(session_id, state);
            }
        }
    }
}

impl WhepOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> WhepOutputTrackStatsReport {
        WhepOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: WhepOutputTrackStatsEvent) {
        match event {
            WhepOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
