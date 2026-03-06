use std::time::Duration;

use crate::stats::{
    WhipOutputStatsEvent, WhipOutputTrackStatsEvent,
    output_reports::{WhipOutputStatsReport, WhipOutputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct WhipOutputState {
    pub video: WhipOutputTrackState,
    pub audio: WhipOutputTrackState,
}

#[derive(Debug)]
pub struct WhipOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl WhipOutputState {
    pub fn new() -> Self {
        Self {
            video: WhipOutputTrackState::new(),
            audio: WhipOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> WhipOutputStatsReport {
        WhipOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: WhipOutputStatsEvent) {
        match event {
            WhipOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            WhipOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl WhipOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> WhipOutputTrackStatsReport {
        WhipOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: WhipOutputTrackStatsEvent) {
        match event {
            WhipOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
