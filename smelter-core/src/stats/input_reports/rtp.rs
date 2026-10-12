use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::stats::input_reports::audio_mixer::AudioMixerInputStatsReport;

/// Stats report for `RTP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpInputStatsReport {
    /// Stats for the video track. `None` before any video arrives.
    pub video: Option<RtpInputVideoTrackStatsReport>,

    /// Stats for the audio track. `None` before any audio arrives.
    pub audio: Option<RtpInputAudioTrackStatsReport>,
}

/// Stats report for the video track in `RTP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpInputVideoTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the jitter buffer.
    pub jitter_buffer: RtpJitterBufferStatsReport,
}

/// Stats report for the audio track in `RTP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpInputAudioTrackStatsReport {
    /// Bitrate of the media in the 1-second window, calculated from timestamps.
    pub bitrate_1_second: u64,
    /// Bitrate of the media in the 1-minute window, calculated from timestamps.
    pub bitrate_1_minute: u64,

    /// Stats for the jitter buffer.
    pub jitter_buffer: RtpJitterBufferStatsReport,

    /// Stats for the audio mixer.
    pub mixer: AudioMixerInputStatsReport,
}

/// Stats report for `RTP` jitter buffer used in `RTP`, `WHIP` and `WHEP` inputs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpJitterBufferStatsReport {
    /// Total count of packets lost.
    pub packets_lost: u64,
    /// Total count of packets received.
    pub packets_received: u64,

    /// Bitrate in the 1-second window.
    pub bitrate_1_second: u64,
    /// Bitrate in the 1-minute window.
    pub bitrate_1_minute: u64,

    /// Jitter buffer stats in the 10-second window.
    pub last_10_seconds: RtpJitterBufferSlidingWindowStatsReport,
}

/// Stats report for the given time window in the `RTP` jitter buffer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpJitterBufferSlidingWindowStatsReport {
    /// Count of packets lost during the given time window.
    pub packets_lost: u64,
    /// Count of packets received during the given time window.
    pub packets_received: u64,

    /// Measured when packet enters jitter buffer. This value represents how
    /// much time packet has to reach the queue to be processed, before
    /// jitter-buffer reorder/wait is applied.
    pub effective_buffer_on_write_avg_seconds: f64,
    /// Measured when packet enters jitter buffer. This value represents how
    /// much time packet has to reach the queue to be processed, before
    /// jitter-buffer reorder/wait is applied.
    pub effective_buffer_on_write_max_seconds: f64,
    /// Measured when packet enters jitter buffer. This value represents how
    /// much time packet has to reach the queue to be processed, before
    /// jitter-buffer reorder/wait is applied.
    pub effective_buffer_on_write_min_seconds: f64,

    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_on_pop_avg_seconds: f64,
    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_on_pop_max_seconds: f64,
    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_on_pop_min_seconds: f64,

    /// Size of the input buffer.
    pub input_buffer_avg_seconds: f64,
    /// Size of the input buffer.
    pub input_buffer_max_seconds: f64,
    /// Size of the input buffer.
    pub input_buffer_min_seconds: f64,
}
