use crate::stats::{
    input::{audio_mixer::AudioMixerInputStatsState, media::InputMediaStatsState},
    input_reports::{
        Mp4InputAudioTrackStatsReport, Mp4InputStatsReport, Mp4InputVideoTrackStatsReport,
    },
};

#[derive(Debug)]
pub struct Mp4InputState {
    pub media: InputMediaStatsState,
    pub audio_mixer: AudioMixerInputStatsState,
}

impl Mp4InputState {
    pub fn new() -> Self {
        Self {
            media: InputMediaStatsState::new(),
            audio_mixer: AudioMixerInputStatsState::new(),
        }
    }

    pub fn report(&mut self) -> Mp4InputStatsReport {
        let has_audio = self.media.audio.received_any() || self.audio_mixer.received_any();
        Mp4InputStatsReport {
            video: self
                .media
                .video
                .received_any()
                .then(|| Mp4InputVideoTrackStatsReport {
                    bitrate_1_second: self.media.video.bitrate_1_second(),
                    bitrate_1_minute: self.media.video.bitrate_1_minute(),
                }),
            audio: has_audio.then(|| Mp4InputAudioTrackStatsReport {
                bitrate_1_second: self.media.audio.bitrate_1_second(),
                bitrate_1_minute: self.media.audio.bitrate_1_minute(),
                mixer: self.audio_mixer.report(),
            }),
        }
    }
}
