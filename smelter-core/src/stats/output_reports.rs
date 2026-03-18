use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatsReport {
    Whep(WhepOutputStatsReport),
    Whip(WhipOutputStatsReport),
    Hls(HlsOutputStatsReport),
    Mp4(Mp4OutputStatsReport),
    Rtmp(RtmpOutputStatsReport),
    Rtp(RtpOutputStatsReport),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepOutputStatsReport {
    pub video: WhepOutputTrackStatsReport,
    pub audio: WhepOutputTrackStatsReport,
    pub connected_peers: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhepOutputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhipOutputStatsReport {
    pub video: WhipOutputTrackStatsReport,
    pub audio: WhipOutputTrackStatsReport,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WhipOutputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsOutputStatsReport {
    pub video: HlsOutputTrackStatsReport,
    pub audio: HlsOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HlsOutputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4OutputStatsReport {
    pub video: Mp4OutputTrackStatsReport,
    pub audio: Mp4OutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Mp4OutputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpOutputStatsReport {
    pub video: RtmpOutputTrackStatsReport,
    pub audio: RtmpOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtmpOutputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpOutputStatsReport {
    pub video: RtpOutputTrackStatsReport,
    pub audio: RtpOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RtpOutputTrackStatsReport {
    pub bitrate_1_second: u64,
    pub bitrate_1_minute: u64,
}
