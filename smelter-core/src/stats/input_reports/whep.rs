use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::stats::input_reports::{
    audio_mixer::AudioMixerInputStatsReport, rtp::RtpJitterBufferStatsReport,
};

/// Stats report for `WHEP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepInputStatsReport {
    /// Stats for the video track. `None` before any video arrives.
    pub video: Option<WhepInputVideoTrackStatsReport>,

    /// Stats for the audio track. `None` before any audio arrives.
    pub audio: Option<WhepInputAudioTrackStatsReport>,
}

/// Stats report for the video track in `WHEP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepInputVideoTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the jitter buffer.
    pub jitter_buffer: RtpJitterBufferStatsReport,
}

/// Stats report for the audio track in `WHEP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepInputAudioTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the jitter buffer.
    pub jitter_buffer: RtpJitterBufferStatsReport,

    /// Stats for the audio mixer.
    pub mixer: AudioMixerInputStatsReport,
}
