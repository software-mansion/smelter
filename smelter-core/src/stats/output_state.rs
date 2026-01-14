use crate::{
    OutputProtocolKind,
    stats::{OutputStatsEvent, output_reports::OutputStatsReport},
};

#[derive(Debug)]
pub enum OutputStatsState {
    Whep,
}

impl OutputStatsState {
    pub fn new(kind: OutputProtocolKind) -> Self {
        match kind {
            OutputProtocolKind::Whep => todo!(),
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
            Self::Whep => todo!(),
        }
    }

    pub fn handle_event(&mut self, event: OutputStatsEvent) {
        todo!()
    }
}
