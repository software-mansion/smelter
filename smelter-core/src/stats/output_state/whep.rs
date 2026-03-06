use std::time::Duration;

use crate::stats::{
    WhepOutputStatsEvent, WhepOutputTrackStatsEvent,
    output_reports::{
        WhepOutputStatsReport, WhepOutputTrackSlidingWindowStatsReport, WhepOutputTrackStatsReport,
    },
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct WhepOutputState {
    pub video: WhepOutputTrackState,
    pub audio: WhepOutputTrackState,
}

#[derive(Debug)]
pub struct WhepOutputTrackState {
    pub bitrate_10_secs: SlidingWindowValue<u64>,
}

impl WhepOutputState {
    pub fn new() -> Self {
        Self {
            video: WhepOutputTrackState::new(),
            audio: WhepOutputTrackState::new(),
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
        }
    }
}

impl WhepOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> WhepOutputTrackStatsReport {
        WhepOutputTrackStatsReport {
            last_10_seconds: WhepOutputTrackSlidingWindowStatsReport {
                bitrate_avg: self.bitrate_10_secs.sum()
                    / self.bitrate_10_secs.window_size().as_secs(),
            },
        }
    }

    pub fn handle_event(&mut self, event: WhepOutputTrackStatsEvent) {
        match event {
            WhepOutputTrackStatsEvent::ChunkSize(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_10_secs.push(chunk_size_bits);
            }
        }
    }
}
