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
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepOutputTrackStatsReport {
    pub last_10_seconds: WhepOutputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepOutputTrackSlidingWindowStatsReport {
    pub bitrate_avg: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhipOutputStatsReport {
    pub video: WhipOutputTrackStatsReport,
    pub audio: WhipOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhipOutputTrackStatsReport {
    pub last_10_seconds: WhipOutputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhipOutputTrackSlidingWindowStatsReport {
    pub bitrate_avg: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HlsOutputStatsReport {
    pub video: HlsOutputTrackStatsReport,
    pub audio: HlsOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HlsOutputTrackStatsReport {
    pub last_10_seconds: HlsOutputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct HlsOutputTrackSlidingWindowStatsReport {
    pub bitrate_avg: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Mp4OutputStatsReport {
    pub video: Mp4OutputTrackStatsReport,
    pub audio: Mp4OutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Mp4OutputTrackStatsReport {
    pub last_10_seconds: Mp4OutputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Mp4OutputTrackSlidingWindowStatsReport {
    pub bitrate_avg: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtmpOutputStatsReport {
    pub video: RtmpOutputTrackStatsReport,
    pub audio: RtmpOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtmpOutputTrackStatsReport {
    pub last_10_seconds: RtmpOutputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtmpOutputTrackSlidingWindowStatsReport {
    pub bitrate_avg: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtpOutputStatsReport {
    pub video: RtpOutputTrackStatsReport,
    pub audio: RtpOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtpOutputTrackStatsReport {
    pub last_10_seconds: RtpOutputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtpOutputTrackSlidingWindowStatsReport {
    pub bitrate_avg: u64,
}
