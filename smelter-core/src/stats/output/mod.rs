use crate::{
    MediaKind, OutputProtocolKind,
    stats::{
        output::hls::HlsOutputState, output::mp4::Mp4OutputState, output::rtmp::RtmpOutputState,
        output::rtp::RtpOutputState, output::whep::WhepOutputState, output::whip::WhipOutputState,
        output_reports::OutputStatsReport,
    },
};

use tracing::error;

pub(super) mod hls;
pub(super) mod mp4;
pub(super) mod rtmp;
pub(super) mod rtp;
pub(super) mod whep;
pub(super) mod whip;

pub(crate) use hls::{HlsOutputStatsEvent, HlsOutputTrackStatsEvent};
pub(crate) use mp4::{Mp4OutputStatsEvent, Mp4OutputTrackStatsEvent};
pub(crate) use rtmp::{RtmpOutputStatsEvent, RtmpOutputTrackStatsEvent};
pub(crate) use rtp::RtpOutputStatsEvent;
pub(crate) use whep::{WhepOutputStatsEvent, WhepOutputTrackStatsEvent};
pub(crate) use whip::{WhipOutputStatsEvent, WhipOutputTrackStatsEvent};

#[derive(Debug, Clone, Copy)]
pub(crate) enum StatsTrackKind {
    Video,
    Audio,
}

impl From<MediaKind> for StatsTrackKind {
    fn from(value: MediaKind) -> Self {
        match value {
            MediaKind::Video(_) => Self::Video,
            MediaKind::Audio(_) => Self::Audio,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum OutputStatsEvent {
    Whep(WhepOutputStatsEvent),
    Whip(WhipOutputStatsEvent),
    Hls(HlsOutputStatsEvent),
    Mp4(Mp4OutputStatsEvent),
    Rtmp(RtmpOutputStatsEvent),

    #[allow(unused)]
    Rtp(RtpOutputStatsEvent),
}

impl From<&OutputStatsEvent> for OutputProtocolKind {
    fn from(value: &OutputStatsEvent) -> Self {
        match value {
            OutputStatsEvent::Whep(_) => Self::Whep,
            OutputStatsEvent::Whip(_) => Self::Whip,
            OutputStatsEvent::Hls(_) => Self::Hls,
            OutputStatsEvent::Mp4(_) => Self::Mp4,
            OutputStatsEvent::Rtmp(_) => Self::Rtmp,
            OutputStatsEvent::Rtp(_) => Self::Rtp,
        }
    }
}

#[derive(Debug)]
pub enum OutputStatsState {
    Whep(WhepOutputState),
    Whip(WhipOutputState),
    Hls(HlsOutputState),
    Mp4(Mp4OutputState),
    Rtmp(RtmpOutputState),
    Rtp(RtpOutputState),
}

impl OutputStatsState {
    pub fn new(kind: OutputProtocolKind) -> Self {
        match kind {
            OutputProtocolKind::Whep => OutputStatsState::Whep(WhepOutputState::new()),
            OutputProtocolKind::Whip => OutputStatsState::Whip(WhipOutputState::new()),
            OutputProtocolKind::Hls => OutputStatsState::Hls(HlsOutputState::new()),
            OutputProtocolKind::Mp4 => OutputStatsState::Mp4(Mp4OutputState::new()),
            OutputProtocolKind::Rtp => OutputStatsState::Rtp(RtpOutputState::new()),
            OutputProtocolKind::Rtmp => OutputStatsState::Rtmp(RtmpOutputState::new()),
            OutputProtocolKind::RawDataChannel => unimplemented!(),
            OutputProtocolKind::EncodedDataChannel => unimplemented!(),
        }
    }

    pub fn report(&mut self) -> OutputStatsReport {
        match self {
            Self::Whep(state) => OutputStatsReport::Whep(state.report()),
            Self::Whip(state) => OutputStatsReport::Whip(state.report()),
            Self::Hls(state) => OutputStatsReport::Hls(state.report()),
            Self::Mp4(state) => OutputStatsReport::Mp4(state.report()),
            Self::Rtmp(state) => OutputStatsReport::Rtmp(state.report()),
            Self::Rtp(state) => OutputStatsReport::Rtp(state.report()),
        }
    }

    pub fn handle_event(&mut self, event: OutputStatsEvent) {
        match (self, event) {
            (OutputStatsState::Whep(state), OutputStatsEvent::Whep(event)) => {
                state.handle_event(event)
            }
            (OutputStatsState::Whip(state), OutputStatsEvent::Whip(event)) => {
                state.handle_event(event)
            }
            (OutputStatsState::Hls(state), OutputStatsEvent::Hls(event)) => {
                state.handle_event(event)
            }
            (OutputStatsState::Mp4(state), OutputStatsEvent::Mp4(event)) => {
                state.handle_event(event)
            }
            (OutputStatsState::Rtmp(state), OutputStatsEvent::Rtmp(event)) => {
                state.handle_event(event)
            }
            (OutputStatsState::Rtp(state), OutputStatsEvent::Rtp(event)) => {
                state.handle_event(event)
            }
            (state, event) => {
                error!(?state, ?event, "Wrong event type for output.")
            }
        }
    }
}
