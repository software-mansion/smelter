use tracing::error;

pub mod hls;
pub mod mp4;
pub mod rtmp;
pub mod rtp;
pub mod whep;
pub mod whip;

use crate::{
    InputProtocolKind,
    stats::{
        InputStatsEvent,
        input_reports::InputStatsReport,
        input_state::{
            hls::HlsInputState, mp4::Mp4InputState, rtmp::RtmpInputState, whep::WhepInputState,
            whip::WhipInputState,
        },
    },
};

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
