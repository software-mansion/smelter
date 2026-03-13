#![allow(unused)]
use std::time::Duration;

use smelter_render::OutputId;

use crate::{
    Ref,
    stats::{
        StatsTrackKind,
        output_reports::{RtpOutputStatsReport, RtpOutputTrackStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

use super::OutputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtpOutputStatsEvent {
    Video(RtpOutputTrackStatsEvent),
    Audio(RtpOutputTrackStatsEvent),
}

impl RtpOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Rtp(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtpOutputTrackStatsEvent {
    BytesSent(usize),
}

impl RtpOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => RtpOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => RtpOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug)]
pub struct RtpOutputState {
    pub video: RtpOutputTrackState,
    pub audio: RtpOutputTrackState,
}

#[derive(Debug)]
pub struct RtpOutputTrackState {
    pub bitrate_1_sec: SlidingWindowValue<u64>,
    pub bitrate_1_min: SlidingWindowValue<u64>,
}

impl RtpOutputState {
    pub fn new() -> Self {
        Self {
            video: RtpOutputTrackState::new(),
            audio: RtpOutputTrackState::new(),
        }
    }

    pub fn report(&mut self) -> RtpOutputStatsReport {
        RtpOutputStatsReport {
            video: self.video.report(),
            audio: self.audio.report(),
        }
    }

    pub fn handle_event(&mut self, event: RtpOutputStatsEvent) {
        match event {
            RtpOutputStatsEvent::Video(track_event) => self.video.handle_event(track_event),
            RtpOutputStatsEvent::Audio(track_event) => self.audio.handle_event(track_event),
        }
    }
}

impl RtpOutputTrackState {
    pub fn new() -> Self {
        Self {
            bitrate_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            bitrate_1_min: SlidingWindowValue::new(Duration::from_mins(1)),
        }
    }

    pub fn report(&mut self) -> RtpOutputTrackStatsReport {
        RtpOutputTrackStatsReport {
            bitrate_1_second: self.bitrate_1_sec.sum() / self.bitrate_1_sec.window_size().as_secs(),

            bitrate_1_minute: self.bitrate_1_min.sum() / self.bitrate_1_min.window_size().as_secs(),
        }
    }

    pub fn handle_event(&mut self, event: RtpOutputTrackStatsEvent) {
        match event {
            RtpOutputTrackStatsEvent::BytesSent(chunk_size_bytes) => {
                let chunk_size_bits = 8 * chunk_size_bytes as u64;
                self.bitrate_1_sec.push(chunk_size_bits);
                self.bitrate_1_min.push(chunk_size_bits);
            }
        }
    }
}
