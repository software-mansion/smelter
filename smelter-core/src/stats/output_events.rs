use smelter_render::OutputId;

use crate::{OutputProtocolKind, Ref, stats::StatsEvent};

#[derive(Debug, Clone, Copy)]
pub(crate) enum StatsTrackKind {
    Video,
    Audio,
}

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
    PeerConnected,
    PeerDisconnected,

    Video(WhepOutputTrackStatsEvent),
    Audio(WhepOutputTrackStatsEvent),
}

impl WhepOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Whep(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepOutputTrackStatsEvent {
    PacketSent,
    NackReceived,
    ChunkSize(u64),
}

impl WhepOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => WhepOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => WhepOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}
