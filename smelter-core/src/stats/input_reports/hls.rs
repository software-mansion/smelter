use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::stats::input_reports::{
    audio_mixer::AudioMixerInputStatsReport, sync::InputSyncTrackStatsReport,
};

/// Stats report for `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputStatsReport {
    /// Stats for the video track. `None` before any video arrives.
    pub video: Option<HlsInputVideoTrackStatsReport>,

    /// Stats for the audio track. `None` before any audio arrives.
    pub audio: Option<HlsInputAudioTrackStatsReport>,
}

/// Stats report for the video track in `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputVideoTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the synchronization. `None` when the track is not active.
    pub sync: Option<InputSyncTrackStatsReport>,
}

/// Stats report for the audio track in `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputAudioTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the synchronization. `None` when the track is not active.
    pub sync: Option<InputSyncTrackStatsReport>,

    /// Stats for the audio mixer.
    pub mixer: AudioMixerInputStatsReport,
}
