use std::time::Duration;

use smelter_render::OutputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{WhepOutputStatsReport, WhepOutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepOutputStatsEvent {
    Video(WhepOutputTrackStatsEvent),
    Audio(WhepOutputTrackStatsEvent),
}

impl WhepOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Whep(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepOutputTrackStatsEvent {
    BytesSent(usize),
}

impl WhepOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => WhepOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => WhepOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct WhepOutputState {
    pub video: WhepOutputTrackState,
    pub audio: WhepOutputTrackState,
}

#[derive(Debug)]
pub struct WhepOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl WhepOutputState {
    pub fn new() -> Self {
        Self {
            video: WhepOutputTrackState::new(),
            audio: WhepOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> WhepOutputStatsReport {
        WhepOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: WhepOutputStatsEvent) {
        match event {
            WhepOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            WhepOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl WhepOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> WhepOutputTrackStatsReport {
        WhepOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: WhepOutputTrackStatsEvent) {
        match event {
            WhepOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
