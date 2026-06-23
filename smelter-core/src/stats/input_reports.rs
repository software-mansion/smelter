use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Stats report for inputs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputStatsReport {
    Rtp(RtpInputStatsReport),
    Whip(WhipInputStatsReport),
    Whep(WhepInputStatsReport),
    Hls(HlsInputStatsReport),
    Rtmp(RtmpInputStatsReport),
    Mp4(Mp4InputStatsReport),
}

/// Stats report for `RTP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpInputStatsReport {
    /// Stats for the video track.
    pub video_rtp: RtpJitterBufferStatsReport,

    /// Stats for the audio track (jitter buffer + per-input audio mixer).
    pub audio: RtpAudioInputStatsReport,
}

/// Stats report for `WHIP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhipInputStatsReport {
    /// Stats for the video track.
    pub video_rtp: RtpJitterBufferStatsReport,

    /// Stats for the audio track (jitter buffer + per-input audio mixer).
    pub audio: RtpAudioInputStatsReport,
}

/// Stats report for `WHEP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepInputStatsReport {
    /// Stats for the video track.
    pub video_rtp: RtpJitterBufferStatsReport,

    /// Stats for the audio track (jitter buffer + per-input audio mixer).
    pub audio: RtpAudioInputStatsReport,
}

/// Combined stats for the audio track of an `RTP` / `WHIP` / `WHEP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpAudioInputStatsReport {
    /// RTP-side jitter buffer stats.
    pub rtp: RtpJitterBufferStatsReport,
    /// Per-input audio mixer (resampler / drift correction) stats.
    pub mixer: AudioMixerStatsReport,
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

/// Stats report for `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpInputStatsReport {
    /// Stats for the video track.
    pub video: RtmpInputTrackStatsReport,

    /// Stats for the audio track (track stats + per-input audio mixer).
    pub audio: RtmpAudioInputStatsReport,
}

/// Combined stats for the audio track of an `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpAudioInputStatsReport {
    /// Per-track RTMP audio stats.
    pub track: RtmpInputTrackStatsReport,
    /// Per-input audio mixer (resampler / drift correction) stats.
    pub mixer: AudioMixerStatsReport,
}

/// Stats report for a track in `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpInputTrackStatsReport {
    /// Bitrate in the 1-second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the 1-minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4InputStatsReport {
    /// Stats for the video track.
    pub video: Mp4InputTrackStatsReport,

    /// Stats for the audio track (track stats + per-input audio mixer).
    pub audio: Mp4AudioInputStatsReport,
}

/// Combined stats for the audio track of an `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4AudioInputStatsReport {
    /// Per-track MP4 audio stats.
    pub track: Mp4InputTrackStatsReport,
    /// Per-input audio mixer (resampler / drift correction) stats.
    pub mixer: AudioMixerStatsReport,
}

/// Stats report for a track in `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4InputTrackStatsReport {
    /// Bitrate in the 1-second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the 1-minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputStatsReport {
    /// Stats for the video track.
    pub video: HlsInputTrackStatsReport,

    /// Stats for the audio track (track stats + per-input audio mixer).
    pub audio: HlsAudioInputStatsReport,
}

/// Combined stats for the audio track of an `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsAudioInputStatsReport {
    /// Per-track HLS audio stats.
    pub track: HlsInputTrackStatsReport,
    /// Per-input audio mixer (resampler / drift correction) stats.
    pub mixer: AudioMixerStatsReport,
}

/// Stats report for a track in the `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputTrackStatsReport {
    /// Total count of the packets received.
    pub packets_received: u64,
    /// Total count of discontinuities between packet timestamps.
    pub discontinuities_detected: u32,

    /// Bitrate in the 1-second window.
    pub bitrate_1_second: u64,
    /// Bitrate in the 1-minute window.
    pub bitrate_1_minute: u64,

    /// Track stats in the 10-second window.
    pub last_10_seconds: HlsInputTrackSlidingWindowStatsReport,
}

/// Stats report for the per-input audio mixer (resampler + drift correction).
///
/// The audio mixer runs once per input audio track. It compensates for the
/// difference between the input clock and the mixing clock by stretching,
/// squashing, dropping, or zero-padding samples; this report describes how
/// much it had to work.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AudioMixerStatsReport {
    /// Total count of discontinuities (input gaps that exceeded the
    /// stretch/squash range and forced a resampler reset) since the input
    /// was registered.
    pub discontinuities_total: u32,

    /// Audio-mixer stats in the 1-second window.
    pub last_1_second: AudioMixerSlidingWindowStatsReport,

    /// Audio-mixer stats in the 10-second window.
    pub last_10_seconds: AudioMixerSlidingWindowStatsReport,
}

/// Stats report for the given time window in the per-input audio mixer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AudioMixerSlidingWindowStatsReport {
    /// Average drift between the input buffer's earliest PTS and the
    /// PTS the mixer asked for. Positive = input is behind the request
    /// (stretching), negative = input is ahead (squashing).
    pub drift_avg_seconds: f64,
    /// Minimum (most-negative) drift observed in the window.
    pub drift_min_seconds: f64,
    /// Maximum (most-positive) drift observed in the window.
    pub drift_max_seconds: f64,

    /// Average duration of audio held in the resampler input buffer
    /// (i.e. pending input samples not yet fed to the resampler).
    pub buffer_duration_avg_seconds: f64,
    /// Minimum buffer duration observed in the window.
    pub buffer_duration_min_seconds: f64,
    /// Maximum buffer duration observed in the window.
    pub buffer_duration_max_seconds: f64,

    /// Count of resampler discontinuities (forced resets) in the window.
    pub discontinuities_count: u32,
}

/// Stats report for the given time window in the `HLS` input track.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputTrackSlidingWindowStatsReport {
    /// Count of packets received during the given time window.
    pub packets_received: u64,

    /// Count of discontinuities between packet timestamps
    /// during the given time window.
    pub discontinuities_detected: u32,

    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_avg_seconds: f64,
    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_max_seconds: f64,
    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_min_seconds: f64,

    /// Size of the input buffer.
    pub input_buffer_avg_seconds: f64,
    /// Size of the input buffer.
    pub input_buffer_max_seconds: f64,
    /// Size of the input buffer.
    pub input_buffer_min_seconds: f64,
}
