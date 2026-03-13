use std::time::Duration;

use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        input_reports::{Mp4InputStatsReport, Mp4InputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Mp4InputStatsEvent {
    Video(Mp4InputTrackStatsEvent),
    Audio(Mp4InputTrackStatsEvent),
}

impl Mp4InputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Mp4(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Mp4InputTrackStatsEvent {
    BytesReceived(usize),
}

impl Mp4InputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        input_ref: &Ref<InputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => Mp4InputStatsEvent::Video(self).into_event(input_ref),
            StatsTrackKind::Audio => Mp4InputStatsEvent::Audio(self).into_event(input_ref),
        }
    }
}

#[derive(Debug)]
pub struct Mp4InputState {
    pub video: Mp4InputTrackState,
    pub audio: Mp4InputTrackState,
}

#[derive(Debug)]
pub struct Mp4InputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl Mp4InputState {
    pub fn new() -> Self {
        Self {
            video: Mp4InputTrackState::new(),
            audio: Mp4InputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> Mp4InputStatsReport {
        Mp4InputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4InputStatsEvent) {
        match event {
            Mp4InputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            Mp4InputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl Mp4InputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> Mp4InputTrackStatsReport {
        Mp4InputTrackStatsReport {
            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_1_minute: self.bitrate_1_min.sum() / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4InputTrackStatsEvent) {
        match event {
            Mp4InputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
