use crate::{
    OutputProtocolKind,
    stats::{
        OutputStatsEvent,
        output_reports::OutputStatsReport,
        output_state::{
            hls::HlsOutputState, mp4::Mp4OutputState, rtmp::RtmpOutputState, rtp::RtpOutputState,
            whep::WhepOutputState, whip::WhipOutputState,
        },
    },
};

use tracing::error;

pub mod hls;
pub mod mp4;
pub mod rtmp;
pub mod rtp;
pub mod whep;
pub mod whip;

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
                error!(?state, ?event, "Wrong event type for input")
            }
        }
    }
}
