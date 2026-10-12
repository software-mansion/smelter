use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        input::{
            audio_mixer::AudioMixerInputStatsState, media::InputMediaStatsState,
            sync::InputSyncState,
        },
        input_reports::{
            RtmpInputAudioTrackStatsReport, RtmpInputStatsReport, RtmpInputVideoTrackStatsReport,
        },
        state::StatsEvent,
    },
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpInputStatsEvent {
    ConnectionEstablished,
    ConnectionClosed,
}

impl RtmpInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Rtmp(self),
        }
    }
}

#[derive(Debug)]
pub struct RtmpInputState {
    pub connected: bool,
    pub sync: InputSyncState,
    pub media: InputMediaStatsState,
    pub audio_mixer: AudioMixerInputStatsState,
}

impl RtmpInputState {
    pub fn new() -> Self {
        Self {
            connected: false,
            sync: InputSyncState::new(),
            media: InputMediaStatsState::new(),
            audio_mixer: AudioMixerInputStatsState::new(),
        }
    }

    pub fn report(&mut self) -> RtmpInputStatsReport {
        let has_video = self.sync.video.was_added() || self.media.video.received_any();
        let has_audio = self.sync.audio.was_added()
            || self.media.audio.received_any()
            || self.audio_mixer.received_any();
        RtmpInputStatsReport {
            is_connected: self.connected,
            video: has_video.then(|| RtmpInputVideoTrackStatsReport {
                bitrate_1_second: self.media.video.bitrate_1_second(),
                bitrate_1_minute: self.media.video.bitrate_1_minute(),
                sync: self.sync.video.report(),
            }),
            audio: has_audio.then(|| RtmpInputAudioTrackStatsReport {
                bitrate_1_second: self.media.audio.bitrate_1_second(),
                bitrate_1_minute: self.media.audio.bitrate_1_minute(),
                sync: self.sync.audio.report(),
                mixer: self.audio_mixer.report(),
            }),
        }
    }

    pub fn handle_event(&mut self, event: RtmpInputStatsEvent) {
        match event {
            RtmpInputStatsEvent::ConnectionEstablished => {
                self.connected = true;
                self.sync.reset();
            }
            RtmpInputStatsEvent::ConnectionClosed => {
                self.connected = false;
                self.sync.reset();
            }
        }
    }
}
