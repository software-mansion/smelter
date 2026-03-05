use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatsReport {
    Whep(WhepOutputStatsReport),
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepOutputStatsReport {
    pub peers_connected: u64,
    pub video: WhepOutputTrackStatsReport,
    pub audio: WhepOutputTrackStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepOutputTrackStatsReport {
    pub packets_sent: u64,
    pub nacks_received: u64,
    pub last_10_seconds: WhepOutputTrackSlidingWindowStatsReport,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WhepOutputTrackSlidingWindowStatsReport {
    pub packets_sent: u64,
    pub nacks_received: u64,
    pub bitrate_avg: u64,
}
