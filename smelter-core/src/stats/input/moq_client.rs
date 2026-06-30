use std::time::Duration;

use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        input_reports::{MoqClientInputStatsReport, MoqClientInputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqClientInputStatsEvent {
    Video(MoqClientInputTrackStatsEvent),
    Audio(MoqClientInputTrackStatsEvent),
}

impl MoqClientInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::MoqClient(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqClientInputTrackStatsEvent {
    BytesReceived(usize),
}

impl MoqClientInputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        input_ref: &Ref<InputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => MoqClientInputStatsEvent::Video(self).into_event(input_ref),
            StatsTrackKind::Audio => MoqClientInputStatsEvent::Audio(self).into_event(input_ref),
        }
    }
}

#[derive(Debug)]
pub struct MoqClientInputState {
    pub video: MoqClientInputTrackState,
    pub audio: MoqClientInputTrackState,
}

#[derive(Debug)]
pub struct MoqClientInputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl MoqClientInputState {
    pub fn new() -> Self {
        Self {
            video: MoqClientInputTrackState::new(),
            audio: MoqClientInputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> MoqClientInputStatsReport {
        MoqClientInputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: MoqClientInputStatsEvent) {
        match event {
            MoqClientInputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            MoqClientInputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl MoqClientInputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> MoqClientInputTrackStatsReport {
        MoqClientInputTrackStatsReport {
            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_1_minute: self.bitrate_1_min.sum() / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: MoqClientInputTrackStatsEvent) {
        match event {
            MoqClientInputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
