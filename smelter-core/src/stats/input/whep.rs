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
            WhepInputAudioTrackStatsReport, WhepInputStatsReport, WhepInputVideoTrackStatsReport,
        },
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
    pub media: InputMediaStatsState,
    pub audio_mixer: AudioMixerInputStatsState,
}

impl WhepInputState {
    pub fn new() -> Self {
        Self {
            video: RtpJitterBufferState::new(),
            audio: RtpJitterBufferState::new(),
            media: InputMediaStatsState::new(),
            audio_mixer: AudioMixerInputStatsState::new(),
        }
    }

    pub fn handle_event(&mut self, event: WhepInputStatsEvent) {
        match event {
            WhepInputStatsEvent::VideoRtp(event) => self.video.handle_event(event),
            WhepInputStatsEvent::AudioRtp(event) => self.audio.handle_event(event),
        }
    }

    pub fn report(&mut self) -> WhepInputStatsReport {
        let has_video = self.video.received_any() || self.media.video.received_any();
        let has_audio = self.audio.received_any()
            || self.media.audio.received_any()
            || self.audio_mixer.received_any();
        WhepInputStatsReport {
            video: has_video.then(|| WhepInputVideoTrackStatsReport {
                bitrate_1_second: self.media.video.bitrate_1_second(),
                bitrate_1_minute: self.media.video.bitrate_1_minute(),
                jitter_buffer: self.video.report(),
            }),
            audio: has_audio.then(|| WhepInputAudioTrackStatsReport {
                bitrate_1_second: self.media.audio.bitrate_1_second(),
                bitrate_1_minute: self.media.audio.bitrate_1_minute(),
                jitter_buffer: self.audio.report(),
                mixer: self.audio_mixer.report(),
            }),
        }
    }
}
