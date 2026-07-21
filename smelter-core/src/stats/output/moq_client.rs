use std::time::Duration;

use smelter_render::OutputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{MoqClientOutputStatsReport, MoqClientOutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqClientOutputStatsEvent {
    Video(MoqClientOutputTrackStatsEvent),
    Audio(MoqClientOutputTrackStatsEvent),
}

impl MoqClientOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::MoqClient(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoqClientOutputTrackStatsEvent {
    BytesSent(usize),
}

impl MoqClientOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => MoqClientOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => MoqClientOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct MoqClientOutputState {
    pub video: MoqClientOutputTrackState,
    pub audio: MoqClientOutputTrackState,
}

#[derive(Debug)]
pub struct MoqClientOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl MoqClientOutputState {
    pub fn new() -> Self {
        Self {
            video: MoqClientOutputTrackState::new(),
            audio: MoqClientOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> MoqClientOutputStatsReport {
        MoqClientOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: MoqClientOutputStatsEvent) {
        match event {
            MoqClientOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            MoqClientOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl MoqClientOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> MoqClientOutputTrackStatsReport {
        MoqClientOutputTrackStatsReport {
            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_1_minute: self.bitrate_1_min.sum() / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: MoqClientOutputTrackStatsEvent) {
        match event {
            MoqClientOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
