use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputStatsReport {
    Whip(WhipInputStatsReport),
    Whep(WhepInputStatsReport),
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhipInputStatsReport {
    pub video_rtp: RtpJitterBufferStatsReport,
    pub audio_rtp: RtpJitterBufferStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepInputStatsReport {
    pub video_rtp: RtpJitterBufferStatsReport,
    pub audio_rtp: RtpJitterBufferStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtpJitterBufferStatsReport {
    pub packets_lost: u64,
    pub packets_received: u64,
    pub last_10_secs: RtpJitterBufferSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtpJitterBufferSlidingWindowStatsReport {
    pub packets_lost: u64,
    pub packets_received: u64,

    /// Measured when packet leaves jitter buffer. This value represents
    /// how much time packet has to reach the queue to be processed.
    pub effective_buffer_avg_secs: f64,
    pub effective_buffer_max_secs: f64,
    pub effective_buffer_min_secs: f64,

    /// Size of the InputBuffer
    pub input_buffer_avg_secs: f64,
    pub input_buffer_max_secs: f64,
    pub input_buffer_min_secs: f64,
}
