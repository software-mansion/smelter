use std::time::Duration;

use smelter_render::InputId;

use crate::{InputProtocolKind, Ref, stats::state::StatsEvent};

#[derive(Debug, Clone, Copy)]
pub(crate) enum InputStatsEvent {
    Whip(WhipInputStatsEvent),
    Whep(WhepInputStatsEvent),
}

impl From<&InputStatsEvent> for InputProtocolKind {
    fn from(value: &InputStatsEvent) -> Self {
        match value {
            InputStatsEvent::Whip(_) => InputProtocolKind::Whip,
            InputStatsEvent::Whep(_) => InputProtocolKind::Whep,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhipInputStatsEvent {
    VideoRtp(RtpJitterBufferStatsEvent),
    AudioRtp(RtpJitterBufferStatsEvent),
}

impl WhipInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Whip(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepInputStatsEvent {
    VideoRtp(RtpJitterBufferStatsEvent),
    AudioRtp(RtpJitterBufferStatsEvent),
}

impl WhepInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Whep(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtpJitterBufferStatsEvent {
    RtpPacketLost(u64),
    RtpPacketReceived,
    EffectiveBuffer(Duration),
}
