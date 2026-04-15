use std::time::Duration;

use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        input_reports::{
            HlsInputStatsReport, HlsInputTrackSlidingWindowStatsReport, HlsInputTrackStatsReport,
        },
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsInputStatsEvent {
    Video(HlsInputTrackStatsEvent),
    Audio(HlsInputTrackStatsEvent),
}

impl HlsInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Hls(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsInputTrackStatsEvent {
    PacketReceived,
    DiscontinuityDetected,
    BytesReceived(usize),
    EffectiveBuffer(Duration),
    InputBufferSize(Duration),
}

#[derive(Debug)]
pub struct HlsInputState {
    pub video: HlsInputTrackState,
    pub audio: HlsInputTrackState,
}

#[derive(Debug)]
pub struct HlsInputTrackState {
    pub packets_received: u64,
    pub packets_received_10_secs: SlidingWindowValue<u64>,

    pub discontinuities_detected: u32,
    pub discontinuities_detected_10_secs: SlidingWindowValue<u32>,

    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,

    pub effective_buffer_10_secs: SlidingWindowValue<Duration>,
    pub input_buffer_10_secs: SlidingWindowValue<Duration>,
}

impl HlsInputState {
    pub fn new() -> Self {
        let video = HlsInputTrackState::new();
        let audio = HlsInputTrackState::new();

        Self { video, audio }
    }

    pub fn report(&mut self) -> HlsInputStatsReport {
        let video_report = self.video.report();
        let audio_report = self.audio.report();

        HlsInputStatsReport {
            video: video_report,
            audio: audio_report,
        }
    }

    pub fn handle_event(&mut self, event: HlsInputStatsEvent) {
        match event {
            HlsInputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            HlsInputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
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

            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),

            effective_buffer_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            input_buffer_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn report(&mut self) -> HlsInputTrackStatsReport {
        HlsInputTrackStatsReport {
            packets_received: self.packets_received,
            discontinuities_detected: self.discontinuities_detected,

            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_1_minute: self.bitrate_1_min.sum() / self.bitrate_1_min.window_size().as_secs(),

            last_10_seconds: HlsInputTrackSlidingWindowStatsReport {
                packets_received: self.packets_received_10_secs.sum(),
                discontinuities_detected: self.discontinuities_detected_10_secs.sum(),

                effective_buffer_avg_seconds: self.effective_buffer_10_secs.avg().as_secs_f64(),
                effective_buffer_max_seconds: self.effective_buffer_10_secs.max().as_secs_f64(),
                effective_buffer_min_seconds: self.effective_buffer_10_secs.min().as_secs_f64(),

                input_buffer_avg_seconds: self.input_buffer_10_secs.avg().as_secs_f64(),
                input_buffer_max_seconds: self.input_buffer_10_secs.max().as_secs_f64(),
                input_buffer_min_seconds: self.input_buffer_10_secs.min().as_secs_f64(),
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
            HlsInputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
