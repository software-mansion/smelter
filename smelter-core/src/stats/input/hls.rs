use crate::stats::{input::sync::InputSyncState, input_reports::HlsInputStatsReport};

#[derive(Debug)]
pub struct HlsInputState {
    pub sync: InputSyncState,
}

impl HlsInputState {
    pub fn new() -> Self {
        Self {
            sync: InputSyncState::new(),
        }
    }

    pub fn report(&mut self) -> HlsInputStatsReport {
        HlsInputStatsReport {
            video: self.sync.video.report(),
            audio: self.sync.audio.report(),
        }
    }
}
