use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Stats report for a track synchronized by the input sync (`RTMP`, `HLS`, `MoQ`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
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

    /// State of the synchronization.
    pub state: LiveSyncTrackState,

    /// Total count of timestamp discontinuities detected.
    pub discontinuities_detected: u32,

    /// How far playback is behind the live edge of the stream (content arriving as fast as the
    /// fastest chunks). It is the latency added by the synchronization, and the value that the
    /// `buffer` option of the input controls. `None` before the track starts.
    pub buffer_seconds: Option<f64>,
    /// Value that `buffer_seconds` is brought to by a re-anchor. It is the desired buffer, or
    /// more when chunks are late on a regular basis. `None` before the track starts.
    pub target_buffer_seconds: Option<f64>,
    /// Smallest `buffer_seconds` that does not start a re-anchor. `None` before the track
    /// starts.
    pub buffer_min_seconds: Option<f64>,
    /// Largest `buffer_seconds` that does not start a re-anchor. `None` before the track
    /// starts.
    pub buffer_max_seconds: Option<f64>,
    /// How much later than the fastest chunks the chunks that are late on a regular basis
    /// arrive. `None` before the track starts.
    pub regular_lateness_seconds: Option<f64>,

    /// Re-anchors since the track was added.
    pub reanchors: LiveSyncReanchorsStatsReport,

    /// Total count of chunks that were dropped: they arrived after the time they should have
    /// been played at, they could not be decoded without a chunk that was dropped, or more than
    /// 120 seconds of content was waiting.
    pub chunks_dropped: u64,
    /// Total count of video frames that arrived after the time they should have been played at
    /// and were shown late.
    pub late_chunks_shown: u64,
    /// Total time the track had nothing to play since it started.
    pub interrupted_seconds: f64,

    /// Content currently held back by the sync.
    pub buffer: LiveSyncBufferStatsReport,

    /// Track stats in the 10-second window.
    pub last_10_seconds: LiveSyncTrackSlidingWindowStatsReport,
}

/// State of the synchronization of a live track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveSyncTrackState {
    /// Chunks are held back until the live edge of the stream is known.
    WaitingForStart,
    Playing,
    /// The track ran out of content to play.
    Interrupted,
    /// A re-anchor is in progress: content is played slightly faster or slower.
    Reanchoring,
}

/// Counts of the re-anchors of a live track. A re-anchor changes the time the content of the
/// stream is played at, to bring `buffer_seconds` back into its range. Every re-anchor is
/// counted once by how it was done (`jumps`, `slews`) and once by its reason (the other fields).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct LiveSyncReanchorsStatsReport {
    /// Re-anchors done at once. Content is skipped or playback pauses.
    pub jumps: u32,
    /// Re-anchors done over time, by playing content slightly faster or slower.
    pub slews: u32,

    /// Chunks that are late on a regular basis had less buffer than the configured minimum.
    pub buffer_too_small: u32,
    /// The buffer was above the configured maximum.
    pub buffer_too_large: u32,
    /// Chunks that are late on a regular basis arrived after the time they should have been
    /// played at.
    pub content_late: u32,
    /// The buffer was outside of its range while nothing was playing.
    pub interrupted: u32,
    /// Buffer that was added for a delay in delivery was taken back after the delay went away.
    pub added_latency_taken_back: u32,
    /// Tracks that were synchronized separately turned out to share a timeline.
    pub tracks_joined: u32,
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
