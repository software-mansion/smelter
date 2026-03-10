use std::time::Duration;

use crate::stats::{
    Mp4InputStatsEvent, Mp4InputTrackStatsEvent,
    input_reports::{Mp4InputStatsReport, Mp4InputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct Mp4InputState {
    pub video: Mp4InputTrackState,
    pub audio: Mp4InputTrackState,
}

#[derive(Debug)]
pub struct Mp4InputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl Mp4InputState {
    pub fn new() -> Self {
        Self {
            video: Mp4InputTrackState::new(),
            audio: Mp4InputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> Mp4InputStatsReport {
        Mp4InputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4InputStatsEvent) {
        match event {
            Mp4InputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            Mp4InputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl Mp4InputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> Mp4InputTrackStatsReport {
        Mp4InputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4InputTrackStatsEvent) {
        match event {
            Mp4InputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
