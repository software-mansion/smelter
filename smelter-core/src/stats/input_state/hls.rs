use std::time::Duration;

use crate::stats::{
    HlsInputStatsEvent, HlsInputTrackStatsEvent,
    input_reports::{
        HlsInputStatsReport, HlsInputTrackSlidingWindowStatsReport, HlsInputTrackStatsReport,
    },
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct HlsInputState {
    pub video: HlsInputTrackState,
    pub audio: HlsInputTrackState,
    pub corrupted_packets_received: u64,
    pub corrupted_packets_received_10_secs: SlidingWindowValue<u64>,
}

#[derive(Debug)]
pub struct HlsInputTrackState {
    pub packets_received: u64,
    pub packets_received_10_secs: SlidingWindowValue<u64>,

    pub discontinuities_detected: u32,
    pub discontinuities_detected_10_secs: SlidingWindowValue<u32>,

    pub bitrate_10_secs: SlidingWindowValue<u64>,

    pub effective_buffer_10_secs: SlidingWindowValue<Duration>,
    pub input_buffer_10_secs: SlidingWindowValue<Duration>,
}

impl HlsInputState {
    pub fn new() -> Self {
        let video = HlsInputTrackState::new();
        let audio = HlsInputTrackState::new();

        Self {
            video,
            audio,
            corrupted_packets_received: 0,
            corrupted_packets_received_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> HlsInputStatsReport {
        let video_report = self.video.report();
        let audio_report = self.audio.report();

        HlsInputStatsReport {
            video: video_report,
            audio: audio_report,
            corrupted_packets_received: self.corrupted_packets_received,
            corrputed_packets_received_last_10_seconds: self
                .corrupted_packets_received_10_secs
                .sum(),
        }
    }

    pub fn handle_event(&mut self, event: HlsInputStatsEvent) {
        match event {
            HlsInputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            HlsInputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
            HlsInputStatsEvent::CorruptedPacketReceived => {
                self.corrupted_packets_received += 1;
                self.corrupted_packets_received_10_secs.push(1);
            }
        }
    }
}

impl HlsInputTrackState {
    pub fn new() -> Self {
        Self {
            packets_received: 0,
            packets_received_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),

            discontinuities_detected: 0,
            discontinuities_detected_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),

            bitrate_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),

            effective_buffer_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            input_buffer_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> HlsInputTrackStatsReport {
        HlsInputTrackStatsReport {
            packets_received: self.packets_received,
            discontinuities_detected: self.discontinuities_detected,
            last_10_seconds: HlsInputTrackSlidingWindowStatsReport {
                packets_received: self.packets_received_10_secs.sum(),
                discontinuities_detected: self.discontinuities_detected_10_secs.sum(),
                bitrate_avg: self.bitrate_10_secs.sum()
                    / self.bitrate_10_secs.window_size().as_secs(),

                effective_buffer_avg_secs: self.effective_buffer_10_secs.avg().as_secs_f64(),
                effective_buffer_max_secs: self.effective_buffer_10_secs.max().as_secs_f64(),
                effective_buffer_min_secs: self.effective_buffer_10_secs.min().as_secs_f64(),

                input_buffer_avg_secs: self.input_buffer_10_secs.avg().as_secs_f64(),
                input_buffer_max_secs: self.input_buffer_10_secs.max().as_secs_f64(),
                input_buffer_min_secs: self.input_buffer_10_secs.min().as_secs_f64(),
            },
        }
    }

    pub fn handle_event(&mut self, event: HlsInputTrackStatsEvent) {
        match event {
            HlsInputTrackStatsEvent::PacketReceived => {
                self.packets_received += 1;
                self.packets_received_10_secs.push(1);
            }
            HlsInputTrackStatsEvent::DiscontinuityDetected => {
                self.discontinuities_detected += 1;
                self.discontinuities_detected_10_secs.push(1);
            }
            HlsInputTrackStatsEvent::EffectiveBuffer(duration) => {
                self.effective_buffer_10_secs.push(duration);
            }
            HlsInputTrackStatsEvent::InputBufferSize(duration) => {
                self.input_buffer_10_secs.push(duration)
            }
            HlsInputTrackStatsEvent::ChunkSize(chunk_size_bytes) => {
                let chunk_size_bits = chunk_size_bytes * 8;
                self.bitrate_10_secs.push(chunk_size_bits);
            }
        }
    }
}
