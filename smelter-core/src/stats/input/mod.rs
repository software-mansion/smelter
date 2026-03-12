use tracing::error;

pub(super) mod hls;
pub(super) mod mp4;
pub(super) mod rtmp;
pub(super) mod rtp;
pub(super) mod whep;
pub(super) mod whip;

use crate::{
    InputProtocolKind,
    stats::{
        input::hls::HlsInputState, input::mp4::Mp4InputState, input::rtmp::RtmpInputState,
        input::whep::WhepInputState, input::whip::WhipInputState, input_reports::InputStatsReport,
    },
};

pub(crate) use hls::{HlsInputStatsEvent, HlsInputTrackStatsEvent};
pub(crate) use mp4::{Mp4InputStatsEvent, Mp4InputTrackStatsEvent};
pub(crate) use rtmp::{RtmpInputStatsEvent, RtmpInputTrackStatsEvent};
pub(crate) use rtp::RtpJitterBufferStatsEvent;
pub(crate) use whep::WhepInputStatsEvent;
pub(crate) use whip::WhipInputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum InputStatsEvent {
    Whip(WhipInputStatsEvent),
    Whep(WhepInputStatsEvent),
    Hls(HlsInputStatsEvent),
    Rtmp(RtmpInputStatsEvent),
    Mp4(Mp4InputStatsEvent),
}

impl From<&InputStatsEvent> for InputProtocolKind {
    fn from(value: &InputStatsEvent) -> Self {
        match value {
            InputStatsEvent::Whip(_) => InputProtocolKind::Whip,
            InputStatsEvent::Whep(_) => InputProtocolKind::Whep,
            InputStatsEvent::Hls(_) => InputProtocolKind::Hls,
            InputStatsEvent::Rtmp(_) => InputProtocolKind::Rtmp,
            InputStatsEvent::Mp4(_) => InputProtocolKind::Mp4,
        }
    }
}

#[derive(Debug)]
pub enum InputStatsState {
    Whip(WhipInputState),
    Whep(WhepInputState),
    Hls(HlsInputState),
    Rtmp(RtmpInputState),
    Mp4(Mp4InputState),
}

impl InputStatsState {
    pub fn new(kind: InputProtocolKind) -> Self {
        match kind {
            InputProtocolKind::Whip => InputStatsState::Whip(WhipInputState::new()),
            InputProtocolKind::Whep => InputStatsState::Whep(WhepInputState::new()),
            InputProtocolKind::Rtp => unimplemented!(),
            InputProtocolKind::Rtmp => InputStatsState::Rtmp(RtmpInputState::new()),
            InputProtocolKind::Mp4 => InputStatsState::Mp4(Mp4InputState::new()),
            InputProtocolKind::Hls => InputStatsState::Hls(HlsInputState::new()),
            InputProtocolKind::V4l2 => unimplemented!(),
            InputProtocolKind::DeckLink => unimplemented!(),
            InputProtocolKind::RawDataChannel => unimplemented!(),
        }
    }

    pub fn handle_event(&mut self, event: InputStatsEvent) {
        match (self, event) {
            (InputStatsState::Whip(state), InputStatsEvent::Whip(event)) => {
                state.handle_event(event)
            }
            (InputStatsState::Whep(state), InputStatsEvent::Whep(event)) => {
                state.handle_event(event)
            }
            (InputStatsState::Hls(state), InputStatsEvent::Hls(event)) => {
                state.handle_event(event);
            }
            (InputStatsState::Rtmp(state), InputStatsEvent::Rtmp(event)) => {
                state.handle_event(event);
            }
            (InputStatsState::Mp4(state), InputStatsEvent::Mp4(event)) => {
                state.handle_event(event);
            }
            (state, event) => {
                error!(?state, ?event, "Wrong event type for input")
            }
        }
    }

    pub fn report(&mut self) -> InputStatsReport {
        match self {
            InputStatsState::Whip(state) => InputStatsReport::Whip(state.report()),
            InputStatsState::Whep(state) => InputStatsReport::Whep(state.report()),
            InputStatsState::Hls(state) => InputStatsReport::Hls(state.report()),
            InputStatsState::Rtmp(state) => InputStatsReport::Rtmp(state.report()),
            InputStatsState::Mp4(state) => InputStatsReport::Mp4(state.report()),
        }
    }
}
