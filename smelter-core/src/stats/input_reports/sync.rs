use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    /// Bitrate of the network traffic in the 1-second window, measured by arrival time. It is
    /// not the bitrate of the media.
    pub bitrate_1_second: u64,
    /// Bitrate of the network traffic in the 1-minute window, measured by arrival time. It is
    /// not the bitrate of the media.
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
    /// Bitrate of the network traffic in the 1-second window, measured by arrival time. It is
    /// not the bitrate of the media.
    pub bitrate_1_second: u64,
    /// Bitrate of the network traffic in the 1-minute window, measured by arrival time. It is
    /// not the bitrate of the media.
    pub bitrate_1_minute: u64,

    /// State of the live edge synchronization.
    pub state: LiveSyncTrackState,

    /// Total count of timestamp discontinuities detected.
    pub discontinuities_detected: u32,

    /// Remaining shift of the playback position until the timestamp offset of the track reaches
    /// its target. Positive when the buffer is being shrunk, negative when it is being grown, zero
    /// when converged. For the secondary track of `started_independent` it is the remaining shift
    /// relative to the leader's final position.
    pub target_offset_distance_seconds: f64,

    /// How far the playback position is behind the pessimistic live edge estimate (content
    /// arriving as slow as the slowest recent chunk). Margin before the playback runs out of
    /// content. Measured against the estimate of both tracks combined in `started_shared` and of
    /// this track alone in `started_independent`. `None` before the track starts.
    pub live_edge_lower_bound_distance_seconds: Option<f64>,

    /// How far the playback position is behind the optimistic live edge estimate (content
    /// arriving as fast as the fastest recent chunk). Total latency introduced by the
    /// synchronization. Measured against the estimate of both tracks combined in `started_shared`
    /// and of this track alone in `started_independent`. `None` before the track starts.
    pub live_edge_upper_bound_distance_seconds: Option<f64>,

    /// Like `live_edge_upper_bound_distance_seconds`, but the estimate only looks back over the
    /// last few seconds instead of the full window, so it reacts to a latency change within
    /// seconds. Never larger than the full-window value; it drops below it when the stream slipped
    /// (content arriving slower than real time) and the full window still remembers the earlier,
    /// faster delivery. `None` before the track starts or when nothing arrived within the window.
    pub live_edge_recent_upper_bound_distance_seconds: Option<f64>,

    /// Content currently held back by the sync.
    pub buffer: LiveSyncBufferStatsReport,

    /// Track stats in the 10-second window.
    pub last_10_seconds: LiveSyncTrackSlidingWindowStatsReport,
}

/// State of the live edge synchronization of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveSyncTrackState {
    /// Chunks are held back until the live edge is estimated.
    WaitingForStart,
    /// Started with the timestamp offset shared with the other track; the buffer is sized from the
    /// live edge of both tracks combined.
    StartedShared,
    /// Started while the tracks are on unrelated timelines. The leader (audio when it runs) sizes
    /// the buffer from its own live edge; the secondary track follows the leader, shifted by the
    /// distance between their live edges.
    StartedIndependent,
}

/// Stats report for the content currently held in the sync buffer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveSyncBufferStatsReport {
    Fifo(FifoBufferStatsReport),
    Jitter(JitterBufferStatsReport),
}

/// Stats report for a FIFO sync buffer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct FifoBufferStatsReport {
    /// Duration of the buffered content.
    pub duration_seconds: f64,
}

/// Stats report for a sync buffer that reorders out-of-order delivery.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct JitterBufferStatsReport {
    /// Duration of the buffered content.
    pub duration_seconds: f64,
    /// Whether the next chunk is held back behind a gap that might still be filled.
    pub waiting_for_gap: bool,
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
