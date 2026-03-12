use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatsReport {
    Whep(WhepOutputStatsReport),
    Whip(WhipOutputStatsReport),
    Hls(HlsOutputStatsReport),
    Mp4(Mp4OutputStatsReport),
    Rtmp(RtmpOutputStatsReport),
    Rtp(RtpOutputStatsReport),
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepOutputStatsReport {
    pub video: WhepOutputTrackStatsReport,
    pub audio: WhepOutputTrackStatsReport,
    pub connected_peers: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepOutputTrackStatsReport {
    pub bitrate_avg_1_second: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhipOutputStatsReport {
    pub video: WhipOutputTrackStatsReport,
    pub audio: WhipOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhipOutputTrackStatsReport {
    pub bitrate_avg_1_second: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HlsOutputStatsReport {
    pub video: HlsOutputTrackStatsReport,
    pub audio: HlsOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HlsOutputTrackStatsReport {
    pub bitrate_avg_1_second: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Mp4OutputStatsReport {
    pub video: Mp4OutputTrackStatsReport,
    pub audio: Mp4OutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Mp4OutputTrackStatsReport {
    pub bitrate_avg_1_second: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtmpOutputStatsReport {
    pub video: RtmpOutputTrackStatsReport,
    pub audio: RtmpOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtmpOutputTrackStatsReport {
    pub bitrate_avg_1_second: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtpOutputStatsReport {
    pub video: RtpOutputTrackStatsReport,
    pub audio: RtpOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtpOutputTrackStatsReport {
    pub bitrate_avg_1_second: u64,
}
