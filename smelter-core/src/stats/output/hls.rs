use std::time::Duration;

use smelter_render::OutputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{HlsOutputStatsReport, HlsOutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsOutputStatsEvent {
    Video(HlsOutputTrackStatsEvent),
    Audio(HlsOutputTrackStatsEvent),
}

impl HlsOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Hls(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsOutputTrackStatsEvent {
    BytesSent(usize),
}

impl HlsOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => HlsOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => HlsOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct HlsOutputState {
    pub video: HlsOutputTrackState,
    pub audio: HlsOutputTrackState,
}

#[derive(Debug)]
pub struct HlsOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
}

impl HlsOutputState {
    pub fn new() -> Self {
        Self {
            video: HlsOutputTrackState::new(),
            audio: HlsOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> HlsOutputStatsReport {
        HlsOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: HlsOutputStatsEvent) {
        match event {
            HlsOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            HlsOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl HlsOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
        }
    }

    pub fn report(&mut self) -> HlsOutputTrackStatsReport {
        HlsOutputTrackStatsReport {
            bitrate_avg_1_second: self.bitrate_1_sec.sum()
                / self.bitrate_1_sec.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: HlsOutputTrackStatsEvent) {
        match event {
            HlsOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
            }
        }
    }
}
