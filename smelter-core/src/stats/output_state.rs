use crate::{
    OutputProtocolKind,
    stats::{
        OutputStatsEvent, output_reports::OutputStatsReport, output_state::whep::WhepOutputState,
    },
};

use tracing::error;

pub mod whep;

#[derive(Debug)]
pub enum OutputStatsState {
    Whep(WhepOutputState),
}

impl OutputStatsState {
    pub fn new(kind: OutputProtocolKind) -> Self {
        match kind {
            OutputProtocolKind::Whep => OutputStatsState::Whep(WhepOutputState::new()),
            OutputProtocolKind::Whip => unimplemented!(),
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
        }
    }

    pub fn handle_event(&mut self, event: OutputStatsEvent) {
        match (self, event) {
            (OutputStatsState::Whep(state), OutputStatsEvent::Whep(event)) => {
                state.handle_event(event)
            }
            (state, event) => {
                error!(?state, ?event, "Wrong event type for input")
            }
        }
    }
}
