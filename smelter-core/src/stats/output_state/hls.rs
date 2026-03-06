use std::time::Duration;

use crate::stats::{
    HlsOutputStatsEvent, HlsOutputTrackStatsEvent,
    output_reports::{
        HlsOutputStatsReport, HlsOutputTrackSlidingWindowStatsReport, HlsOutputTrackStatsReport,
    },
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct HlsOutputState {
    pub video: HlsOutputTrackState,
    pub audio: HlsOutputTrackState,
}

#[derive(Debug)]
pub struct HlsOutputTrackState {
    pub bitrate_10_secs: SlidingWindowValue<u64>,
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
            bitrate_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> HlsOutputTrackStatsReport {
        HlsOutputTrackStatsReport {
            last_10_seconds: HlsOutputTrackSlidingWindowStatsReport {
                bitrate_avg: self.bitrate_10_secs.sum()
                    / self.bitrate_10_secs.window_size().as_secs(),
            },
        }
    }

    pub fn handle_event(&mut self, event: HlsOutputTrackStatsEvent) {
        match event {
            HlsOutputTrackStatsEvent::ChunkSize(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_10_secs.push(chunk_size_bits);
            }
        }
    }
}