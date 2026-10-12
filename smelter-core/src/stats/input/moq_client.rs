use crate::stats::{
    input::{
        audio_mixer::AudioMixerInputStatsState, media::InputMediaStatsState, sync::InputSyncState,
    },
    input_reports::{
        MoqClientInputAudioTrackStatsReport, MoqClientInputStatsReport,
        MoqClientInputVideoTrackStatsReport,
    },
};

#[derive(Debug)]
pub struct MoqClientInputState {
    pub sync: InputSyncState,
    pub media: InputMediaStatsState,
    pub audio_mixer: AudioMixerInputStatsState,
}

impl MoqClientInputState {
    pub fn new() -> Self {
        Self {
            sync: InputSyncState::new(),
            media: InputMediaStatsState::new(),
            audio_mixer: AudioMixerInputStatsState::new(),
        }
    }

    pub fn report(&mut self) -> MoqClientInputStatsReport {
        let has_video = self.sync.video.was_added() || self.media.video.received_any();
        let has_audio = self.sync.audio.was_added()
            || self.media.audio.received_any()
            || self.audio_mixer.received_any();
        MoqClientInputStatsReport {
            video: has_video.then(|| MoqClientInputVideoTrackStatsReport {
                bitrate_1_second: self.media.video.bitrate_1_second(),
                bitrate_1_minute: self.media.video.bitrate_1_minute(),
                sync: self.sync.video.report(),
            }),
            audio: has_audio.then(|| MoqClientInputAudioTrackStatsReport {
                bitrate_1_second: self.media.audio.bitrate_1_second(),
                bitrate_1_minute: self.media.audio.bitrate_1_minute(),
                sync: self.sync.audio.report(),
                mixer: self.audio_mixer.report(),
            }),
        }
    }
}
