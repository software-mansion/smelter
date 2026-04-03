use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Stats report for outputs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputStatsReport {
    Whep(WhepOutputStatsReport),
    Whip(WhipOutputStatsReport),
    Hls(HlsOutputStatsReport),
    Mp4(Mp4OutputStatsReport),
    Rtmp(RtmpOutputStatsReport),
    Rtp(RtpOutputStatsReport),
}

/// Stats report for `WHEP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepOutputStatsReport {
    /// Stats for the video track.
    pub video: WhepOutputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: WhepOutputTrackStatsReport,

    /// Count of currently connected peers.
    pub connected_peers: u64,
}

/// Stats report for a track in the `WHEP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepOutputTrackStatsReport {
    /// Bitrate in the one second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the one minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for the `WHIP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhipOutputStatsReport {
    /// Stats for the video track.
    pub video: WhipOutputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: WhipOutputTrackStatsReport,

    /// Indicator if the output is connected to the `WHIP` server.
    pub is_connected: bool,
}

/// Stats report for a track in the `WHIP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhipOutputTrackStatsReport {
    /// Bitrate in the one second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the one minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for the `HLS` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsOutputStatsReport {
    /// Stats for the video track.
    pub video: HlsOutputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: HlsOutputTrackStatsReport,
}

/// Stats report for a track in the `HLS` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsOutputTrackStatsReport {
    /// Bitrate in the one second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the one minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for the `MP4` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4OutputStatsReport {
    /// Stats for the video track.
    pub video: Mp4OutputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: Mp4OutputTrackStatsReport,
}

/// Stats report for a track in the `MP4` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4OutputTrackStatsReport {
    /// Bitrate in the one second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the one minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for the `RTMP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpOutputStatsReport {
    /// Stats for the video track.
    pub video: RtmpOutputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: RtmpOutputTrackStatsReport,
}

/// Stats report for a track in the `RTMP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpOutputTrackStatsReport {
    /// Bitrate in the one second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the one minute window.
    pub bitrate_1_minute: u64,
}

/// Stats report for the `RTP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpOutputStatsReport {
    /// Stats for the video track.
    pub video: RtpOutputTrackStatsReport,

    /// Stats for the audio track.
    pub audio: RtpOutputTrackStatsReport,
}

/// Stats report for a track in the `RTP` output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpOutputTrackStatsReport {
    /// Bitrate in the one second window.
    pub bitrate_1_second: u64,

    /// Bitrate in the one minute window.
    pub bitrate_1_minute: u64,
}
