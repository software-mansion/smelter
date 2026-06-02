use std::time::Duration;

use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        input_reports::{MoqServerInputStatsReport, MoqServerInputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqServerInputStatsEvent {
    Video(MoqServerInputTrackStatsEvent),
    Audio(MoqServerInputTrackStatsEvent),
}

impl MoqServerInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::MoqServer(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqServerInputTrackStatsEvent {
    BytesReceived(usize),
}

impl MoqServerInputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        input_ref: &Ref<InputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => MoqServerInputStatsEvent::Video(self).into_event(input_ref),
            StatsTrackKind::Audio => MoqServerInputStatsEvent::Audio(self).into_event(input_ref),
        }
    }
}

#[derive(Debug)]
pub struct MoqServerInputState {
    pub video: MoqServerInputTrackState,
    pub audio: MoqServerInputTrackState,
}

#[derive(Debug)]
pub struct MoqServerInputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl MoqServerInputState {
    pub fn new() -> Self {
        Self {
            video: MoqServerInputTrackState::new(),
            audio: MoqServerInputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> MoqServerInputStatsReport {
        MoqServerInputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: MoqServerInputStatsEvent) {
        match event {
            MoqServerInputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            MoqServerInputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl MoqServerInputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> MoqServerInputTrackStatsReport {
        MoqServerInputTrackStatsReport {
            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_1_minute: self.bitrate_1_min.sum() / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: MoqServerInputTrackStatsEvent) {
        match event {
            MoqServerInputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
