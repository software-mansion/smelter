use std::time::Duration;

use smelter_render::OutputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{RtmpOutputStatsReport, RtmpOutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpOutputStatsEvent {
    Video(RtmpOutputTrackStatsEvent),
    Audio(RtmpOutputTrackStatsEvent),
}

impl RtmpOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Rtmp(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpOutputTrackStatsEvent {
    BytesSent(usize),
}

impl RtmpOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => RtmpOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => RtmpOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct RtmpOutputState {
    pub video: RtmpOutputTrackState,
    pub audio: RtmpOutputTrackState,
}

#[derive(Debug)]
pub struct RtmpOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl RtmpOutputState {
    pub fn new() -> Self {
        Self {
            video: RtmpOutputTrackState::new(),
            audio: RtmpOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> RtmpOutputStatsReport {
        RtmpOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpOutputStatsEvent) {
        match event {
            RtmpOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            RtmpOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl RtmpOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> RtmpOutputTrackStatsReport {
        RtmpOutputTrackStatsReport {
            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_1_minute: self.bitrate_1_min.sum()
                / self.bitrate_1_min.actual_window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: RtmpOutputTrackStatsEvent) {
        match event {
            RtmpOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
