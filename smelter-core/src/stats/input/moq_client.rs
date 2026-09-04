use crate::stats::{input::sync::InputSyncState, input_reports::MoqClientInputStatsReport};

#[derive(Debug)]
pub struct MoqClientInputState {
    pub sync: InputSyncState,
}

impl MoqClientInputState {
    pub fn new() -> Self {
        Self {
            sync: InputSyncState::new(),
        }
    }

    pub fn report(&mut self) -> MoqClientInputStatsReport {
        MoqClientInputStatsReport {
            video: self.sync.video.report(),
            audio: self.sync.audio.report(),
        }
    }
}
