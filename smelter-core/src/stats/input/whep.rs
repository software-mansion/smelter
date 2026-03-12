use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        input::rtp::{RtpJitterBufferState, RtpJitterBufferStatsEvent},
        input_reports::WhepInputStatsReport,
        state::StatsEvent,
    },
};

use super::InputStatsEvent;

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
