use std::time::Duration;

use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        input_reports::{RtmpInputStatsReport, RtmpInputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::InputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpInputStatsEvent {
    Video(RtmpInputTrackStatsEvent),
    Audio(RtmpInputTrackStatsEvent),
}

impl RtmpInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Rtmp(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpInputTrackStatsEvent {
    BytesReceived(usize),
}

impl RtmpInputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        input_ref: &Ref<InputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => RtmpInputStatsEvent::Video(self).into_event(input_ref),
            StatsTrackKind::Audio => RtmpInputStatsEvent::Audio(self).into_event(input_ref),
        }
    }
}

#[derive(Debug)]
pub struct RtmpInputState {
    pub video: RtmpInputTrackState,
    pub audio: RtmpInputTrackState,
}

#[derive(Debug)]
pub struct RtmpInputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl RtmpInputState {
    pub fn new() -> Self {
        Self {
            video: RtmpInputTrackState::new(),
            audio: RtmpInputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> RtmpInputStatsReport {
        RtmpInputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpInputStatsEvent) {
        match event {
            RtmpInputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            RtmpInputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl RtmpInputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> RtmpInputTrackStatsReport {
        RtmpInputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_avg_1_minute: self.bitrate_1_min.sum()
                / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpInputTrackStatsEvent) {
        match event {
            RtmpInputTrackStatsEvent::BytesReceived(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
