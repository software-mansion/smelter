use std::time::Duration;

use crate::stats::{
    WhepOutputStatsEvent, WhepOutputTrackStatsEvent,
    output_reports::{WhepOutputStatsReport, WhepOutputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct WhepOutputState {
    pub video: WhepOutputTrackState,
    pub audio: WhepOutputTrackState,

    pub peers_connected: u32,
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

            peers_connected: 0,
        }
    }

    pub fn report(&mut self) -> WhepOutputStatsReport {
        WhepOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: WhepOutputStatsEvent) {
        match event {
            WhepOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            WhepOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
            WhepOutputStatsEvent::PeerConnected => self.peers_connected += 1,
            WhepOutputStatsEvent::PeerDisconnected => self.peers_connected -= 1,
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
