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
    Whip(WhipOutputStatsEvent),
    Hls(HlsOutputStatsEvent),
    Mp4(Mp4OutputStatsEvent),
    Rtmp(RtmpOutputStatsEvent),
}

impl From<&OutputStatsEvent> for OutputProtocolKind {
    fn from(value: &OutputStatsEvent) -> Self {
        match value {
            OutputStatsEvent::Whep(_) => Self::Whep,
            OutputStatsEvent::Whip(_) => Self::Whip,
            OutputStatsEvent::Hls(_) => Self::Hls,
            OutputStatsEvent::Mp4(_) => Self::Mp4,
            OutputStatsEvent::Rtmp(_) => Self::Rtmp,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepOutputStatsEvent {
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

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhipOutputStatsEvent {
    Video(WhipOutputTrackStatsEvent),
    Audio(WhipOutputTrackStatsEvent),
}

impl WhipOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Whip(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhipOutputTrackStatsEvent {
    ChunkSize(u64),
}

impl WhipOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => WhipOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => WhipOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsOutputStatsEvent {
    Video(HlsOutputTrackStatsEvent),
    Audio(HlsOutputTrackStatsEvent),
}

impl HlsOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Hls(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsOutputTrackStatsEvent {
    ChunkSize(u64),
}

impl HlsOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => HlsOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => HlsOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Mp4OutputStatsEvent {
    Video(Mp4OutputTrackStatsEvent),
    Audio(Mp4OutputTrackStatsEvent),
}

impl Mp4OutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Mp4(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Mp4OutputTrackStatsEvent {
    ChunkSize(u64),
}

impl Mp4OutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => Mp4OutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => Mp4OutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpOutputStatsEvent {
    Video(RtmpOutputTrackStatsEvent),
    Audio(RtmpOutputTrackStatsEvent),
}

impl RtmpOutputStatsEvent {
    pub fn into_event(self, output_ref: &Ref<OutputId>) -> StatsEvent {
        StatsEvent::Output {
            output_ref: output_ref.clone(),
            event: OutputStatsEvent::Rtmp(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpOutputTrackStatsEvent {
    ChunkSize(u64),
}

impl RtmpOutputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        output_ref: &Ref<OutputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => RtmpOutputStatsEvent::Video(self).into_event(output_ref),
            StatsTrackKind::Audio => RtmpOutputStatsEvent::Audio(self).into_event(output_ref),
        }
    }
}
