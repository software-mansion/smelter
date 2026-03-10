#![allow(unused)]
use std::time::Duration;

use crate::stats::{
    RtpOutputStatsEvent, RtpOutputTrackStatsEvent,
    output_reports::{RtpOutputStatsReport, RtpOutputTrackStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct RtpOutputState {
    pub video: RtpOutputTrackState,
    pub audio: RtpOutputTrackState,
}

#[derive(Debug)]
pub struct RtpOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl RtpOutputState {
    pub fn new() -> Self {
        Self {
            video: RtpOutputTrackState::new(),
            audio: RtpOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> RtpOutputStatsReport {
        RtpOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: RtpOutputStatsEvent) {
        match event {
            RtpOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            RtpOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl RtpOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> RtpOutputTrackStatsReport {
        RtpOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: RtpOutputTrackStatsEvent) {
        match event {
            RtpOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
