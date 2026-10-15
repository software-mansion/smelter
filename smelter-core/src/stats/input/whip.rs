use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        input::{
            audio_mixer::AudioMixerInputStatsState,
            media::InputMediaStatsState,
            rtp::{RtpJitterBufferState, RtpJitterBufferStatsEvent},
        },
        input_reports::{
            WhipInputAudioTrackStatsReport, WhipInputStatsReport, WhipInputVideoTrackStatsReport,
        },
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
    pub media: InputMediaStatsState,
    pub audio_mixer: AudioMixerInputStatsState,
}

impl WhipInputState {
    pub fn new() -> Self {
        Self {
            video: RtpJitterBufferState::new(),
            audio: RtpJitterBufferState::new(),
            media: InputMediaStatsState::new(),
            audio_mixer: AudioMixerInputStatsState::new(),
        }
    }

    pub fn handle_event(&mut self, event: WhipInputStatsEvent) {
        match event {
            WhipInputStatsEvent::VideoRtp(event) => self.video.handle_event(event),
            WhipInputStatsEvent::AudioRtp(event) => self.audio.handle_event(event),
        }
    }

    pub fn report(&mut self) -> WhipInputStatsReport {
        let has_video = self.video.received_any() || self.media.video.received_any();
        let has_audio = self.audio.received_any()
            || self.media.audio.received_any()
            || self.audio_mixer.received_any();
        WhipInputStatsReport {
            video: has_video.then(|| WhipInputVideoTrackStatsReport {
                bitrate_1_second: self.media.video.bitrate_1_second(),
                bitrate_1_minute: self.media.video.bitrate_1_minute(),
                jitter_buffer: self.video.report(),
            }),
            audio: has_audio.then(|| WhipInputAudioTrackStatsReport {
                bitrate_1_second: self.media.audio.bitrate_1_second(),
                bitrate_1_minute: self.media.audio.bitrate_1_minute(),
                jitter_buffer: self.audio.report(),
                mixer: self.audio_mixer.report(),
            }),
        }
    }
}
