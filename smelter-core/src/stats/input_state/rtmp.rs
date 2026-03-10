use std::time::Duration;

use crate::stats::{
    RtmpInputStatsEvent, RtmpInputTrackStatsEvent,
    input_reports::{RtmpInputStatsReport, RtmpInputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct RtmpInputState {
    pub video: RtmpInputTrackState,
    pub audio: RtmpInputTrackState,
}

#[derive(Debug)]
pub struct RtmpInputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl RtmpInputState {
    pub fn new() -> Self {
        Self {
            video: RtmpInputTrackState::new(),
            audio: RtmpInputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> RtmpInputStatsReport {
        RtmpInputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpInputStatsEvent) {
        match event {
            RtmpInputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            RtmpInputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl RtmpInputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> RtmpInputTrackStatsReport {
        RtmpInputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpInputTrackStatsEvent) {
        match event {
            RtmpInputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
