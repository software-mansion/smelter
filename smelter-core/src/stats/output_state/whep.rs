use std::time::Duration;

use crate::stats::{
    WhepOutputStatsEvent, WhepOutputTrackStatsEvent,
    output_reports::{
        WhepOutputStatsReport, WhepOutputTrackStatsReport, WhepOutputsTrackSlidingWindowStatsReport,
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
    pub packets_sent: u64,
    pub nacks_received: u64,

    pub packets_sent_10_secs: SlidingWindowValue<u64>,
    pub nacks_received_10_secs: SlidingWindowValue<u64>,

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
            packets_sent: 0,
            packets_sent_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),

            nacks_received: 0,
            nacks_received_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),

            bitrate_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> WhepOutputTrackStatsReport {
        WhepOutputTrackStatsReport {
            packets_sent: self.packets_sent,
            nacks_received: self.nacks_received,

            last_10_seconds: WhepOutputsTrackSlidingWindowStatsReport {
                packets_sent: self.packets_sent_10_secs.sum(),
                nacks_received: self.nacks_received_10_secs.sum(),

                bitrate_avg: self.bitrate_10_secs.sum()
                    / self.bitrate_10_secs.window_size().as_secs(),
            },
        }
    }

    pub fn handle_event(&mut self, event: WhepOutputTrackStatsEvent) {
        match event {
            WhepOutputTrackStatsEvent::PacketSent => {
                self.packets_sent += 1;
                self.packets_sent_10_secs.push(1);
            }
            WhepOutputTrackStatsEvent::NackReceived => {
                self.nacks_received += 1;
                self.nacks_received_10_secs.push(1);
            }
            WhepOutputTrackStatsEvent::ChunkSize(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_10_secs.push(chunk_size_bits);
            }
        }
    }
}
