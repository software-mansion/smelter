use crate::{
    OutputProtocolKind,
    stats::{
        OutputStatsEvent,
        output_reports::OutputStatsReport,
        output_state::{whep::WhepOutputState, whip::WhipOutputState},
    },
};

use tracing::error;

pub mod whep;
pub mod whip;

#[derive(Debug)]
pub enum OutputStatsState {
    Whep(WhepOutputState),
    Whip(WhipOutputState),
}

impl OutputStatsState {
    pub fn new(kind: OutputProtocolKind) -> Self {
        match kind {
            OutputProtocolKind::Whep => OutputStatsState::Whep(WhepOutputState::new()),
            OutputProtocolKind::Whip => OutputStatsState::Whip(WhipOutputState::new()),
            OutputProtocolKind::Hls => unimplemented!(),
            OutputProtocolKind::Mp4 => unimplemented!(),
            OutputProtocolKind::Rtp => unimplemented!(),
            OutputProtocolKind::Rtmp => unimplemented!(),
            OutputProtocolKind::RawDataChannel => unimplemented!(),
            OutputProtocolKind::EncodedDataChannel => unimplemented!(),
        }
    }

    pub fn report(&mut self) -> OutputStatsReport {
        match self {
            Self::Whep(state) => OutputStatsReport::Whep(state.report()),
            Self::Whip(state) => OutputStatsReport::Whip(state.report()),
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
            #[allow(unreachable_patterns)]
            (state, event) => {
                error!(?state, ?event, "Wrong event type for input")
            }
        }
    }
}
