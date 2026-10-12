use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::stats::input_reports::audio_mixer::AudioMixerInputStatsReport;

/// Stats report for `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4InputStatsReport {
    /// Stats for the video track. `None` before any video arrives.
    pub video: Option<Mp4InputVideoTrackStatsReport>,

    /// Stats for the audio track. `None` before any audio arrives.
    pub audio: Option<Mp4InputAudioTrackStatsReport>,
}

/// Stats report for the video track in `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4InputVideoTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,
}

/// Stats report for the audio track in `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4InputAudioTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the audio mixer.
    pub mixer: AudioMixerInputStatsReport,
}
