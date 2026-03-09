use std::time::Duration;

use crate::stats::{
    RtmpOutputStatsEvent, RtmpOutputTrackStatsEvent,
    output_reports::{RtmpOutputStatsReport, RtmpOutputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct RtmpOutputState {
    pub video: RtmpOutputTrackState,
    pub audio: RtmpOutputTrackState,
}

#[derive(Debug)]
pub struct RtmpOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl RtmpOutputState {
    pub fn new() -> Self {
        Self {
            video: RtmpOutputTrackState::new(),
            audio: RtmpOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> RtmpOutputStatsReport {
        RtmpOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpOutputStatsEvent) {
        match event {
            RtmpOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            RtmpOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl RtmpOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> RtmpOutputTrackStatsReport {
        RtmpOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpOutputTrackStatsEvent) {
        match event {
            RtmpOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
