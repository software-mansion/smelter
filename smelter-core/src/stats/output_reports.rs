use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatsReport {
    Whep(WhepOutputStatsReport),
    Whip(WhipOutputStatsReport),
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
