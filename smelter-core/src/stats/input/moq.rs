use std::time::Duration;

use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        input_reports::{MoqInputStatsReport, MoqInputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqInputStatsEvent {
    Video(MoqInputTrackStatsEvent),
    Audio(MoqInputTrackStatsEvent),
}

impl MoqInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Moq(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqInputTrackStatsEvent {
    BytesReceived(usize),
}

impl MoqInputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        input_ref: &Ref<InputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => MoqInputStatsEvent::Video(self).into_event(input_ref),
            StatsTrackKind::Audio => MoqInputStatsEvent::Audio(self).into_event(input_ref),
        }
    }
}

#[derive(Debug)]
pub struct MoqInputState {
    pub video: MoqInputTrackState,
    pub audio: MoqInputTrackState,
}

#[derive(Debug)]
pub struct MoqInputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl MoqInputState {
    pub fn new() -> Self {
        Self {
            video: MoqInputTrackState::new(),
            audio: MoqInputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> MoqInputStatsReport {
        MoqInputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: MoqInputStatsEvent) {
        match event {
            MoqInputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            MoqInputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl MoqInputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> MoqInputTrackStatsReport {
        MoqInputTrackStatsReport {
            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),
            bitrate_1_minute: self.bitrate_1_min.sum() / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: MoqInputTrackStatsEvent) {
        match event {
            MoqInputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
