use std::time::Duration;

use crate::stats::{
    RtpJitterBufferStatsEvent,
    input_reports::{RtpJitterBufferSlidingWindowStatsReport, RtpJitterBufferStatsReport},
    utils::SlidingWindowValue,
};

#[derive(Debug)]
pub struct RtpJitterBufferState {
    pub packets_lost: u64,
    pub packets_lost_10_secs: SlidingWindowValue<u64>,
    pub packets_received: u64,
    pub packets_received_10_secs: SlidingWindowValue<u64>,
    pub effective_buffer_10_secs: SlidingWindowValue<Duration>,
    pub input_buffer_10_secs: SlidingWindowValue<Duration>,

    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl RtpJitterBufferState {
    pub fn new() -> Self {
        Self {
            packets_lost: 0,
            packets_lost_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            packets_received: 0,
            packets_received_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            effective_buffer_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            input_buffer_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn handle_event(&mut self, event: RtpJitterBufferStatsEvent) {
        match event {
            RtpJitterBufferStatsEvent::RtpPacketLost => {
                self.packets_lost += 1;
                self.packets_lost_10_secs.push(1);
            }
            RtpJitterBufferStatsEvent::RtpPacketReceived => {
                self.packets_received += 1;
                self.packets_received_10_secs.push(1);
            }
            RtpJitterBufferStatsEvent::EffectiveBuffer(duration) => {
                self.effective_buffer_10_secs.push(duration);
            }
            RtpJitterBufferStatsEvent::InputBufferSize(duration) => {
                self.input_buffer_10_secs.push(duration);
            }
            RtpJitterBufferStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }

    pub fn report(&mut self) -> RtpJitterBufferStatsReport {
        RtpJitterBufferStatsReport {
            packets_lost: self.packets_lost,
            packets_received: self.packets_received,
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
            last_10_secs: RtpJitterBufferSlidingWindowStatsReport {
                packets_lost: self.packets_lost_10_secs.sum(),
                packets_received: self.packets_received_10_secs.sum(),
                effective_buffer_avg_secs: self.effective_buffer_10_secs.avg().as_secs_f64(),
                effective_buffer_max_secs: self.effective_buffer_10_secs.max().as_secs_f64(),
                effective_buffer_min_secs: self.effective_buffer_10_secs.min().as_secs_f64(),
                input_buffer_avg_secs: self.input_buffer_10_secs.avg().as_secs_f64(),
                input_buffer_max_secs: self.input_buffer_10_secs.max().as_secs_f64(),
                input_buffer_min_secs: self.input_buffer_10_secs.min().as_secs_f64(),
            },
        }
    }
}
