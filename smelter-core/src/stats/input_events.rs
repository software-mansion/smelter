use std::time::Duration;

use smelter_render::InputId;

use crate::{
    InputProtocolKind, Ref,
    stats::{StatsTrackKind, state::StatsEvent},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum InputStatsEvent {
    Whip(WhipInputStatsEvent),
    Whep(WhepInputStatsEvent),
    Hls(HlsInputStatsEvent),
    Rtmp(RtmpInputStatsEvent),
}

impl From<&InputStatsEvent> for InputProtocolKind {
    fn from(value: &InputStatsEvent) -> Self {
        match value {
            InputStatsEvent::Whip(_) => InputProtocolKind::Whip,
            InputStatsEvent::Whep(_) => InputProtocolKind::Whep,
            InputStatsEvent::Hls(_) => InputProtocolKind::Hls,
            InputStatsEvent::Rtmp(_) => InputProtocolKind::Rtmp,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhipInputStatsEvent {
    VideoRtp(RtpJitterBufferStatsEvent),
    AudioRtp(RtpJitterBufferStatsEvent),
}

impl WhipInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Whip(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WhepInputStatsEvent {
    VideoRtp(RtpJitterBufferStatsEvent),
    AudioRtp(RtpJitterBufferStatsEvent),
}

impl WhepInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Whep(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtpJitterBufferStatsEvent {
    RtpPacketLost,
    RtpPacketReceived,
    EffectiveBuffer(Duration),
    InputBufferSize(Duration),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsInputStatsEvent {
    Video(HlsInputTrackStatsEvent),
    Audio(HlsInputTrackStatsEvent),
    CorruptedPacketReceived,
}

impl HlsInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Hls(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpInputStatsEvent {
    Video(RtmpInputTrackStatsEvent),
    Audio(RtmpInputTrackStatsEvent),
}

impl RtmpInputStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::Input {
            input_ref: input_ref.clone(),
            event: InputStatsEvent::Rtmp(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RtmpInputTrackStatsEvent {
    BytesReceived(u64),
}

impl RtmpInputTrackStatsEvent {
    pub(crate) fn into_event(
        self,
        input_ref: &Ref<InputId>,
        track_kind: StatsTrackKind,
    ) -> StatsEvent {
        match track_kind {
            StatsTrackKind::Video => RtmpInputStatsEvent::Video(self).into_event(input_ref),
            StatsTrackKind::Audio => RtmpInputStatsEvent::Audio(self).into_event(input_ref),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HlsInputTrackStatsEvent {
    PacketReceived,
    DiscontinuityDetected,
    ChunkSize(u64),
    EffectiveBuffer(Duration),
    InputBufferSize(Duration),
}
