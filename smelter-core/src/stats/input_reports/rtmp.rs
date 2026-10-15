use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::stats::input_reports::{
    audio_mixer::AudioMixerInputStatsReport, sync::InputSyncTrackStatsReport,
};

/// Stats report for `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpInputStatsReport {
    /// Whether a client is currently connected.
    pub is_connected: bool,

    /// Stats for the video track. `None` before any video arrives.
    pub video: Option<RtmpInputVideoTrackStatsReport>,

    /// Stats for the audio track. `None` before any audio arrives.
    pub audio: Option<RtmpInputAudioTrackStatsReport>,
}

/// Stats report for the video track in `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpInputVideoTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the synchronization. `None` when the track is not active.
    pub sync: Option<InputSyncTrackStatsReport>,
}

/// Stats report for the audio track in `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpInputAudioTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the synchronization. `None` when the track is not active.
    pub sync: Option<InputSyncTrackStatsReport>,

    /// Stats for the audio mixer.
    pub mixer: AudioMixerInputStatsReport,
}
