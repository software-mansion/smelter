use crate::stats::{
    WhipInputStatsEvent, input_reports::WhipInputStatsReport,
    input_state::rtp::RtpJitterBufferState,
};

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
