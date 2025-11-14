use std::time::Duration;

use tracing::error;

use crate::{
    InputProtocolKind,
    stats::{
        InputStatsEvent, RtpJitterBufferStatsEvent, WhepInputStatsEvent, WhipInputStatsEvent,
        input_reports::{
            InputStatsReport, RtpJitterBufferSlidingWindowStatsReport, RtpJitterBufferStatsReport,
            WhepInputStatsReport, WhipInputStatsReport,
        },
        utils::SlidingWindowValue,
    },
};

#[derive(Debug)]
pub enum InputStatsState {
    WhipInput(WhipInputState),
    WhepInput(WhepInputState),
}

impl InputStatsState {
    pub fn new(kind: InputProtocolKind) -> Self {
        match kind {
            InputProtocolKind::Whip => InputStatsState::WhipInput(WhipInputState::new()),
            InputProtocolKind::Whep => InputStatsState::WhepInput(WhepInputState::new()),
            InputProtocolKind::Rtp => panic!(),
            InputProtocolKind::Mp4 => panic!(),
            InputProtocolKind::Hls => panic!(),
            InputProtocolKind::DeckLink => panic!(),
            InputProtocolKind::RawDataChannel => panic!(),
        }
    }

    pub fn handle_event(&mut self, event: InputStatsEvent) {
        match (self, event) {
            (InputStatsState::WhipInput(state), InputStatsEvent::Whip(event)) => {
                state.handle_event(event)
            }
            (InputStatsState::WhepInput(state), InputStatsEvent::Whep(event)) => {
                state.handle_event(event)
            }
            (state, event) => {
                error!(?state, ?event, "Wrong event type for input")
            }
        }
    }

    pub fn report(&mut self) -> InputStatsReport {
        match self {
            InputStatsState::WhipInput(state) => InputStatsReport::Whip(state.report()),
            InputStatsState::WhepInput(state) => InputStatsReport::Whep(state.report()),
        }
    }
}

#[derive(Debug)]
pub struct WhipInputState {
    pub video: RtpJitterBufferState,
    pub audio: RtpJitterBufferState,
}

impl WhipInputState {
    pub fn new() -> Self {
        Self {
            video: RtpJitterBufferState::new(),
            audio: RtpJitterBufferState::new(),
        }
    }

    pub fn handle_event(&mut self, event: WhipInputStatsEvent) {
        match event {
            WhipInputStatsEvent::VideoRtp(event) => self.video.handle_event(event),
            WhipInputStatsEvent::AudioRtp(event) => self.audio.handle_event(event),
        }
    }

    pub fn report(&mut self) -> WhipInputStatsReport {
        WhipInputStatsReport {
            video_rtp: self.video.report(),
            audio_rtp: self.audio.report(),
        }
    }
}

#[derive(Debug)]
pub struct WhepInputState {
    pub video: RtpJitterBufferState,
    pub audio: RtpJitterBufferState,
}

impl WhepInputState {
    pub fn new() -> Self {
        Self {
            video: RtpJitterBufferState::new(),
            audio: RtpJitterBufferState::new(),
        }
    }

    pub fn handle_event(&mut self, event: WhepInputStatsEvent) {
        match event {
            WhepInputStatsEvent::VideoRtp(event) => self.video.handle_event(event),
            WhepInputStatsEvent::AudioRtp(event) => self.audio.handle_event(event),
        }
    }

    pub fn report(&mut self) -> WhepInputStatsReport {
        WhepInputStatsReport {
            video_rtp: self.video.report(),
            audio_rtp: self.audio.report(),
        }
    }
}

#[derive(Debug)]
pub struct RtpJitterBufferState {
    pub packets_lost: u64,
    pub packets_lost_10_secs: SlidingWindowValue<u64>,
    pub packets_received: u64,
    pub packets_received_10_secs: SlidingWindowValue<u64>,
    pub effective_buffer_10_secs: SlidingWindowValue<Duration>,
    pub input_buffer_10_secs: SlidingWindowValue<Duration>,
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
        }
    }

    pub fn handle_event(&mut self, event: RtpJitterBufferStatsEvent) {
        match event {
            RtpJitterBufferStatsEvent::RtpPacketLost(count) => {
                self.packets_lost += 1;
                self.packets_lost_10_secs.push(count);
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
        }
    }

    pub fn report(&mut self) -> RtpJitterBufferStatsReport {
        RtpJitterBufferStatsReport {
            packets_lost: self.packets_lost,
            packets_received: self.packets_received,
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
