use std::time::Duration;

use crate::stats::{
    Mp4OutputStatsEvent, Mp4OutputTrackStatsEvent,
    output_reports::{
        Mp4OutputStatsReport, Mp4OutputTrackSlidingWindowStatsReport, Mp4OutputTrackStatsReport,
    },
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct Mp4OutputState {
    pub video: Mp4OutputTrackState,
    pub audio: Mp4OutputTrackState,
}

#[derive(Debug)]
pub struct Mp4OutputTrackState {
    pub bitrate_10_secs: SlidingWindowValue<u64>,
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
            bitrate_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> Mp4OutputTrackStatsReport {
        Mp4OutputTrackStatsReport {
            last_10_seconds: Mp4OutputTrackSlidingWindowStatsReport {
                bitrate_avg: self.bitrate_10_secs.sum()
                    / self.bitrate_10_secs.window_size().as_secs(),
            },
        }
    }

    pub fn handle_event(&mut self, event: Mp4OutputTrackStatsEvent) {
        match event {
            Mp4OutputTrackStatsEvent::ChunkSize(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_10_secs.push(chunk_size_bits);
            }
        }
    }
}
