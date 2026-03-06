use std::time::Duration;

use crate::stats::{
    RtpOutputStatsEvent, RtpOutputTrackStatsEvent,
    output_reports::{
        RtpOutputStatsReport, RtpOutputTrackSlidingWindowStatsReport, RtpOutputTrackStatsReport,
    },
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct RtpOutputState {
    pub video: RtpOutputTrackState,
    pub audio: RtpOutputTrackState,
}

#[derive(Debug)]
pub struct RtpOutputTrackState {
    pub bitrate_10_secs: SlidingWindowValue<u64>,
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
            bitrate_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> RtpOutputTrackStatsReport {
        RtpOutputTrackStatsReport {
            last_10_seconds: RtpOutputTrackSlidingWindowStatsReport {
                bitrate_avg: self.bitrate_10_secs.sum()
                    / self.bitrate_10_secs.window_size().as_secs(),
            },
        }
    }

    pub fn handle_event(&mut self, event: RtpOutputTrackStatsEvent) {
        match event {
            RtpOutputTrackStatsEvent::ChunkSize(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_10_secs.push(chunk_size_bits);
            }
        }
    }
}
