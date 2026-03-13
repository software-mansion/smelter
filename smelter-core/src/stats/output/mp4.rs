use std::time::Duration;

use smelter_render::OutputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{Mp4OutputStatsReport, Mp4OutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Mp4OutputStatsEvent {
    Video(Mp4OutputTrackStatsEvent),
    Audio(Mp4OutputTrackStatsEvent),
}

impl Mp4OutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Mp4(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Mp4OutputTrackStatsEvent {
    BytesSent(usize),
}

impl Mp4OutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => Mp4OutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => Mp4OutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct Mp4OutputState {
    pub video: Mp4OutputTrackState,
    pub audio: Mp4OutputTrackState,
}

#[derive(Debug)]
pub struct Mp4OutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl Mp4OutputState {
    pub fn new() -> Self {
        Self {
            video: Mp4OutputTrackState::new(),
            audio: Mp4OutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> Mp4OutputStatsReport {
        Mp4OutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4OutputStatsEvent) {
        match event {
            Mp4OutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            Mp4OutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl Mp4OutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> Mp4OutputTrackStatsReport {
        Mp4OutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_avg_1_minute: self.bitrate_1_min.sum()
                / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: Mp4OutputTrackStatsEvent) {
        match event {
            Mp4OutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
