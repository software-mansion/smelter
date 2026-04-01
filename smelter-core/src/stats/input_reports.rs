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

    /// Stats for the audio track.
    pub audio_rtp: RtpJitterBufferStatsReport,
}

/// Stats report for `WHIP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhipInputStatsReport {
    /// Stats for the video track.
    pub video_rtp: RtpJitterBufferStatsReport,

    /// Stats for the audio track.
    pub audio_rtp: RtpJitterBufferStatsReport,
}

/// Stats report for `WHEP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepInputStatsReport {
    /// Stats for the video track.
    pub video_rtp: RtpJitterBufferStatsReport,

    /// Stats for the audio track.
    pub audio_rtp: RtpJitterBufferStatsReport,
}

/// Stats report for `RTP` jitter buffer used in `RTP`, `WHIP` and `WHEP` inputs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpJitterBufferStatsReport {
    /// Total count of packets lost.
    pub packets_lost: u64,
    /// Total count of packets received.
    pub packets_received: u64,

    /// Bitrate from the last second.
    pub bitrate_1_second: u64,
    /// Bitrate from the last minute.
    pub bitrate_1_minute: u64,

    /// Stats from the last 10 seconds.
    pub last_10_seconds: RtpJitterBufferSlidingWindowStatsReport,
}

/// Stats report for the given time window in the `RTP` jitter buffer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpJitterBufferSlidingWindowStatsReport {
    /// Count of packets lost during the given time window.
    pub packets_lost: u64,
    /// Count of packets received during the given time window.
    pub packets_received: u64,

    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_avg_seconds: f64,
    pub effective_buffer_max_seconds: f64,
    pub effective_buffer_min_seconds: f64,

    /// Size of the input buffer.
    pub input_buffer_avg_seconds: f64,
    pub input_buffer_max_seconds: f64,
    pub input_buffer_min_seconds: f64,
}

/// Stats report for `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpInputStatsReport {
    /// Stats for the video track.
    pub video: RtmpInputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: RtmpInputTrackStatsReport,
}

/// Stats report for a track in `RTMP` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpInputTrackStatsReport {
    /// Bitrate from the last second.
    pub bitrate_1_second: u64,

    /// Bitrate from the last minute.
    pub bitrate_1_minute: u64,
}

/// Stats report for `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4InputStatsReport {
    /// Stats for the video track.
    pub video: Mp4InputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: Mp4InputTrackStatsReport,
}

/// Stats report for a track in `MP4` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4InputTrackStatsReport {
    /// Bitrate from the last second.
    pub bitrate_1_second: u64,

    /// Bitrate from the last minute.
    pub bitrate_1_minute: u64,
}

/// Stats report for `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputStatsReport {
    /// Stats for the video track.
    pub video: HlsInputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: HlsInputTrackStatsReport,

    /// Total count of corrupted packets received.
    pub corrupted_packets_received: u64,

    /// Count of corrupted packets received for the last 10 seconds.
    pub corrupted_packets_received_last_10_seconds: u64,
}

/// Stats report for a track in the `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputTrackStatsReport {
    /// Total count of the packets received.
    pub packets_received: u64,
    /// Total count of detected discontinuities between packet timestamps.
    pub discontinuities_detected: u32,

    /// Bitrate from the last second.
    pub bitrate_1_second: u64,
    /// Bitrate from the last minute.
    pub bitrate_1_minute: u64,

    /// Stats from the last 10 seconds.
    pub last_10_seconds: HlsInputTrackSlidingWindowStatsReport,
}

/// Stats report for the given time window in the `HLS` input track.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputTrackSlidingWindowStatsReport {
    /// Count of packets received during the given time window.
    pub packets_received: u64,

    /// Count of detected discontinuities between packet timestamps
    /// during the given time window.
    pub discontinuities_detected: u32,

    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_avg_seconds: f64,
    pub effective_buffer_max_seconds: f64,
    pub effective_buffer_min_seconds: f64,

    /// Size of the input buffer.
    pub input_buffer_avg_seconds: f64,
    pub input_buffer_max_seconds: f64,
    pub input_buffer_min_seconds: f64,
}
