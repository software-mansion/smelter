use crate::stats::{input::sync::InputSyncState, input_reports::MoqServerInputStatsReport};

#[derive(Debug)]
pub struct MoqServerInputState {
    pub sync: InputSyncState,
}

impl MoqServerInputState {
    pub fn new() -> Self {
        Self {
            sync: InputSyncState::new(),
        }
    }

    pub fn report(&mut self) -> MoqServerInputStatsReport {
        MoqServerInputStatsReport {
            video: self.sync.video.report(),
            audio: self.sync.audio.report(),
        }
    }
}
