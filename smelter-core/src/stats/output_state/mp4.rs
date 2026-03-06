use std::time::Duration;

use crate::stats::{
    Mp4OutputStatsEvent, Mp4OutputTrackStatsEvent,
    output_reports::{Mp4OutputStatsReport, Mp4OutputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct Mp4OutputState {
    pub video: Mp4OutputTrackState,
    pub audio: Mp4OutputTrackState,
}

#[derive(Debug)]
pub struct Mp4OutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl Mp4OutputState {
    pub fn new() -> Self {
        Self {
            video: Mp4OutputTrackState::new(),
            audio: Mp4OutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> Mp4OutputStatsReport {
        Mp4OutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4OutputStatsEvent) {
        match event {
            Mp4OutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            Mp4OutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl Mp4OutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> Mp4OutputTrackStatsReport {
        Mp4OutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4OutputTrackStatsEvent) {
        match event {
            Mp4OutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
