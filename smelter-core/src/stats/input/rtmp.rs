use smelter_render::InputId;

use crate::{
    Ref,
    stats::{input::sync::InputSyncState, input_reports::RtmpInputStatsReport, state::StatsEvent},
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpInputStatsEvent {
    ConnectionEstablished,
    ConnectionClosed,
}

impl RtmpInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Rtmp(self),
        }
    }
}

#[derive(Debug)]
pub struct RtmpInputState {
    pub connected: bool,
    pub sync: InputSyncState,
}

impl RtmpInputState {
    pub fn new() -> Self {
        Self {
            connected: false,
            sync: InputSyncState::new(),
        }
    }

    pub fn report(&mut self) -> RtmpInputStatsReport {
        RtmpInputStatsReport {
            is_connected: self.connected,
            video: self.sync.video.report(),
            audio: self.sync.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpInputStatsEvent) {
        match event {
            RtmpInputStatsEvent::ConnectionEstablished => {
                self.connected = true;
                self.sync.reset();
            }
            RtmpInputStatsEvent::ConnectionClosed => {
                self.connected = false;
                self.sync.reset();
            }
        }
    }
}
