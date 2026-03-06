use std::time::Duration;

use crate::stats::{
    HlsOutputStatsEvent, HlsOutputTrackStatsEvent,
    output_reports::{HlsOutputStatsReport, HlsOutputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct HlsOutputState {
    pub video: HlsOutputTrackState,
    pub audio: HlsOutputTrackState,
}

#[derive(Debug)]
pub struct HlsOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl HlsOutputState {
    pub fn new() -> Self {
        Self {
            video: HlsOutputTrackState::new(),
            audio: HlsOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> HlsOutputStatsReport {
        HlsOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: HlsOutputStatsEvent) {
        match event {
            HlsOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            HlsOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl HlsOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> HlsOutputTrackStatsReport {
        HlsOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: HlsOutputTrackStatsEvent) {
        match event {
            HlsOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
