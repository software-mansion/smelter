use crate::stats::{
    WhepInputStatsEvent, input_reports::WhepInputStatsReport,
    input_state::rtp::RtpJitterBufferState,
};

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
