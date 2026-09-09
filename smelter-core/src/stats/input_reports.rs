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
    MoqServer(MoqServerInputStatsReport),
    MoqClient(MoqClientInputStatsReport),
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
    /// Whether a client is currently connected.
    pub is_connected: bool,

    /// Stats for the video track. `None` when the track is not active.
    pub video: Option<InputSyncTrackStatsReport>,

    /// Stats for the audio track. `None` when the track is not active.
    pub audio: Option<InputSyncTrackStatsReport>,
}

/// Stats report for `MoQ` server input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct MoqServerInputStatsReport {
    /// Stats for the video track. `None` when the track is not active.
    pub video: Option<InputSyncTrackStatsReport>,

    /// Stats for the audio track. `None` when the track is not active.
    pub audio: Option<InputSyncTrackStatsReport>,
}

/// Stats report for `MoQ` client input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct MoqClientInputStatsReport {
    /// Stats for the video track. `None` when the track is not active.
    pub video: Option<InputSyncTrackStatsReport>,

    /// Stats for the audio track. `None` when the track is not active.
    pub audio: Option<InputSyncTrackStatsReport>,
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
    /// Bitrate in the 1-second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the 1-minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for `HLS` input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsInputStatsReport {
    /// Stats for the video track. `None` when the track is not active.
    pub video: Option<InputSyncTrackStatsReport>,

    /// Stats for the audio track. `None` when the track is not active.
    pub audio: Option<InputSyncTrackStatsReport>,
}

/// Stats report for a track synchronized by the input sync (`RTMP`, `HLS`, `MoQ`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum InputSyncTrackStatsReport {
    /// Non-live stream, timestamps are normalized to start at zero.
    Simple(SimpleSyncTrackStatsReport),
    /// Live stream, synchronized to the estimated live edge.
    Live(LiveSyncTrackStatsReport),
}

/// Stats report for a track of a non-live stream.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct SimpleSyncTrackStatsReport {
    /// Bitrate in the 1-second window.
    pub bitrate_1_second: u64,
    /// Bitrate in the 1-minute window.
    pub bitrate_1_minute: u64,

    /// State of the synchronization.
    pub state: SimpleSyncTrackState,
}

/// State of the synchronization of a non-live track.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimpleSyncTrackState {
    /// Chunks are held back until the initial buffer is collected.
    InitialBuffering,
    Running,
}

/// Stats report for a track of a live stream.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct LiveSyncTrackStatsReport {
    /// Bitrate in the 1-second window.
    pub bitrate_1_second: u64,
    /// Bitrate in the 1-minute window.
    pub bitrate_1_minute: u64,

    /// State of the live edge synchronization.
    pub state: LiveSyncTrackState,

    /// Total count of timestamp discontinuities detected.
    pub discontinuities_detected: u32,

    /// Remaining shift of the playback position to reach the target buffer.
    /// Positive when the buffer is being shrunk, negative when it is being
    /// grown, zero when converged.
    pub target_offset_distance_seconds: f64,

    /// How far the playback position is behind the pessimistic live edge
    /// estimate (content arriving as slow as the slowest recent chunk).
    /// Margin before the playback runs out of content. `None` before the
    /// track starts.
    pub live_edge_lower_bound_distance_seconds: Option<f64>,

    /// How far the playback position is behind the optimistic live edge
    /// estimate (content arriving as fast as the fastest recent chunk).
    /// Total latency introduced by the synchronization. `None` before the
    /// track starts.
    pub live_edge_upper_bound_distance_seconds: Option<f64>,

    /// Content currently held back by the sync.
    pub buffer: LiveSyncBufferStatsReport,

    /// Track stats in the 10-second window.
    pub last_10_seconds: LiveSyncTrackSlidingWindowStatsReport,
}

/// State of the live edge synchronization of a track.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveSyncTrackState {
    /// Chunks are held back until the live edge is estimated.
    WaitingForStart,
    /// Started, aligned to the live edge shared with the other track.
    StartedShared,
    /// Started with its own live edge, timestamps are unrelated to the other track.
    StartedTrack,
}

/// Stats report for the content currently held in the sync buffer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveSyncBufferStatsReport {
    Fifo(FifoBufferStatsReport),
}

/// Stats report for a FIFO sync buffer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct FifoBufferStatsReport {
    /// Duration of the buffered content.
    pub duration_seconds: f64,
}

/// Stats report for the given time window in a live stream track.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct LiveSyncTrackSlidingWindowStatsReport {
    /// Count of timestamp discontinuities detected during the given time window.
    pub discontinuities_detected: u32,

    /// Measured when chunk enters the sync buffer, using the current
    /// timestamp mapping. This value represents how much time chunk has to
    /// reach the queue to be processed, before any waiting in the sync buffer.
    /// Negative when the chunk is already late. Not measured before the
    /// track starts.
    pub effective_buffer_on_receive_avg_seconds: f64,
    /// Measured when chunk enters the sync buffer, using the current
    /// timestamp mapping. This value represents how much time chunk has to
    /// reach the queue to be processed, before any waiting in the sync buffer.
    /// Negative when the chunk is already late. Not measured before the
    /// track starts.
    pub effective_buffer_on_receive_max_seconds: f64,
    /// Measured when chunk enters the sync buffer, using the current
    /// timestamp mapping. This value represents how much time chunk has to
    /// reach the queue to be processed, before any waiting in the sync buffer.
    /// Negative when the chunk is already late. Not measured before the
    /// track starts.
    pub effective_buffer_on_receive_min_seconds: f64,

    /// Measured when chunk leaves the sync buffer. This value represents
    /// how much time chunk has to reach the queue to be processed.
    /// Negative when the chunk is already late.
    pub effective_buffer_on_output_avg_seconds: f64,
    /// Measured when chunk leaves the sync buffer. This value represents
    /// how much time chunk has to reach the queue to be processed.
    /// Negative when the chunk is already late.
    pub effective_buffer_on_output_max_seconds: f64,
    /// Measured when chunk leaves the sync buffer. This value represents
    /// how much time chunk has to reach the queue to be processed.
    /// Negative when the chunk is already late.
    pub effective_buffer_on_output_min_seconds: f64,
}
