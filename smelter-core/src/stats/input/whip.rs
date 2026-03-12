use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        input::rtp::{RtpJitterBufferState, RtpJitterBufferStatsEvent},
        input_reports::WhipInputStatsReport,
        state::StatsEvent,
    },
};

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

use super::InputStatsEvent;

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
