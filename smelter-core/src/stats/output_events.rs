use smelter_render::OutputId;

use crate::{OutputProtocolKind, Ref, stats::StatsEvent};

#[derive(Debug, Clone, Copy)]
pub(crate) enum OutputStatsEvent {
    Whep(WhepOutputStatsEvent),
}

impl From<&OutputStatsEvent> for OutputProtocolKind {
    fn from(value: &OutputStatsEvent) -> Self {
        match value {
            OutputStatsEvent::Whep(_) => Self::Whep,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepOutputStatsEvent {
    Video(WhepOutputTrackStatsEvent),
    Audio(WhepOutputTrackStatsEvent),
}

// impl WhepOutputStatsEvent {
//     pub fn into_event(self, input_ref: &Ref<OutputId>) -> StatsEvent {
//         StatsEvent
//     }
// }

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepOutputTrackStatsEvent {
    PacketSent,
    NackReceived,
    ChunkSize(u64),
}
