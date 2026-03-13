use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputStatsReport {
    Whip(WhipInputStatsReport),
    Whep(WhepInputStatsReport),
    Hls(HlsInputStatsReport),
    Rtmp(RtmpInputStatsReport),
    Mp4(Mp4InputStatsReport),
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct WhipInputStatsReport {
    pub video_rtp: RtpJitterBufferStatsReport,
    pub audio_rtp: RtpJitterBufferStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct WhepInputStatsReport {
    pub video_rtp: RtpJitterBufferStatsReport,
    pub audio_rtp: RtpJitterBufferStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct RtpJitterBufferStatsReport {
    pub packets_lost: u64,
    pub packets_received: u64,
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,

    pub last_10_seconds: RtpJitterBufferSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct RtpJitterBufferSlidingWindowStatsReport {
    pub packets_lost: u64,
    pub packets_received: u64,

    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_avg_seconds: f64,
    pub effective_buffer_max_seconds: f64,
    pub effective_buffer_min_seconds: f64,

    /// Size of the InputBuffer
    pub input_buffer_avg_seconds: f64,
    pub input_buffer_max_seconds: f64,
    pub input_buffer_min_seconds: f64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct RtmpInputStatsReport {
    pub video: RtmpInputTrackStatsReport,
    pub audio: RtmpInputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct RtmpInputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct Mp4InputStatsReport {
    pub video: Mp4InputTrackStatsReport,
    pub audio: Mp4InputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct Mp4InputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct HlsInputStatsReport {
    pub video: HlsInputTrackStatsReport,
    pub audio: HlsInputTrackStatsReport,
    pub corrupted_packets_received: u64,
    pub corrupted_packets_received_last_10_seconds: u64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct HlsInputTrackStatsReport {
    pub packets_received: u64,
    pub discontinuities_detected: u32,
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,

    pub last_10_seconds: HlsInputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct HlsInputTrackSlidingWindowStatsReport {
    pub packets_received: u64,
    pub discontinuities_detected: u32,

    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_avg_seconds: f64,
    pub effective_buffer_max_seconds: f64,
    pub effective_buffer_min_seconds: f64,

    /// Size of the InputBuffer
    pub input_buffer_avg_seconds: f64,
    pub input_buffer_max_seconds: f64,
    pub input_buffer_min_seconds: f64,
}
