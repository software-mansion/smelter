use tracing::error;

pub(super) mod audio_mixer;
pub(super) mod decklink;
pub(super) mod hls;
pub(super) mod media;
pub(super) mod moq_client;
pub(super) mod mp4;
pub(super) mod raw_data;
pub(super) mod rtmp;
pub(super) mod rtp;
pub(super) mod sync;
pub(super) mod v4l2;
pub(super) mod whep;
pub(super) mod whip;

use crate::{
    InputProtocolKind,
    pipeline::utils::input_sync::TrackKind,
    stats::{
        input::audio_mixer::AudioMixerInputStatsState, input::decklink::DeckLinkInputState,
        input::hls::HlsInputState, input::media::InputMediaStatsState,
        input::moq_client::MoqClientInputState, input::mp4::Mp4InputState,
        input::raw_data::RawDataInputState, input::rtmp::RtmpInputState, input::rtp::RtpInputState,
        input::v4l2::V4l2InputState, input::whep::WhepInputState, input::whip::WhipInputState,
        input_reports::InputStatsReport,
    },
};

pub(crate) use audio_mixer::AudioMixerInputStatsEvent;
pub(crate) use media::InputMediaStatsEvent;
pub(crate) use rtmp::RtmpInputStatsEvent;
pub(crate) use rtp::{RtpInputStatsEvent, RtpJitterBufferStatsEvent};
pub(crate) use sync::{
    InputSyncMode, InputSyncTrackStatsEvent, LiveSyncBufferStats, LiveSyncStatsEvent,
    LiveSyncTrackStateSnapshot, SimpleSyncStatsEvent,
};
pub(crate) use whep::WhepInputStatsEvent;
pub(crate) use whip::WhipInputStatsEvent;

#[derive(Debug, Clone, Copy)]
pub(crate) enum InputStatsEvent {
    Rtp(RtpInputStatsEvent),
    Whip(WhipInputStatsEvent),
    Whep(WhepInputStatsEvent),
    /// Sent by the input sync of `HLS`, `RTMP` and `MoQ` inputs.
    Sync {
        track: TrackKind,
        event: InputSyncTrackStatsEvent,
    },
    Rtmp(RtmpInputStatsEvent),
    /// Sent by decoders for every input.
    Media {
        track: TrackKind,
        event: InputMediaStatsEvent,
    },
    /// Sent by the audio mixer for every input.
    AudioMixer(AudioMixerInputStatsEvent),
}

#[derive(Debug)]
pub enum InputStatsState {
    Rtp(RtpInputState),
    Whip(WhipInputState),
    Whep(WhepInputState),
    Hls(HlsInputState),
    Rtmp(RtmpInputState),
    MoqClient(MoqClientInputState),
    Mp4(Mp4InputState),
    V4l2(V4l2InputState),
    DeckLink(DeckLinkInputState),
    RawData(RawDataInputState),
}

impl InputStatsState {
    pub fn new(kind: InputProtocolKind) -> Self {
        match kind {
            InputProtocolKind::Whip => InputStatsState::Whip(WhipInputState::new()),
            InputProtocolKind::Whep => InputStatsState::Whep(WhepInputState::new()),
            InputProtocolKind::Rtp => InputStatsState::Rtp(RtpInputState::new()),
            InputProtocolKind::Rtmp => InputStatsState::Rtmp(RtmpInputState::new()),
            InputProtocolKind::Mp4 => InputStatsState::Mp4(Mp4InputState::new()),
            InputProtocolKind::Hls => InputStatsState::Hls(HlsInputState::new()),
            InputProtocolKind::MoqClient => InputStatsState::MoqClient(MoqClientInputState::new()),
            InputProtocolKind::V4l2 => InputStatsState::V4l2(V4l2InputState),
            InputProtocolKind::DeckLink => InputStatsState::DeckLink(DeckLinkInputState),
            InputProtocolKind::RawDataChannel => InputStatsState::RawData(RawDataInputState),
        }
    }

    pub fn handle_event(&mut self, event: InputStatsEvent) {
        match (self, event) {
            (InputStatsState::Rtp(state), InputStatsEvent::Rtp(event)) => state.handle_event(event),
            (InputStatsState::Whip(state), InputStatsEvent::Whip(event)) => {
                state.handle_event(event)
            }
            (InputStatsState::Whep(state), InputStatsEvent::Whep(event)) => {
                state.handle_event(event)
            }
            (InputStatsState::Hls(state), InputStatsEvent::Sync { track, event }) => {
                state.sync.handle_event(track, event);
            }
            (InputStatsState::Rtmp(state), InputStatsEvent::Sync { track, event }) => {
                state.sync.handle_event(track, event);
            }
            (InputStatsState::Rtmp(state), InputStatsEvent::Rtmp(event)) => {
                state.handle_event(event);
            }
            (InputStatsState::MoqClient(state), InputStatsEvent::Sync { track, event }) => {
                state.sync.handle_event(track, event);
            }
            (state, InputStatsEvent::Media { track, event }) => {
                if let Some(media) = state.media() {
                    media.handle_event(track, event);
                }
            }
            (state, InputStatsEvent::AudioMixer(event)) => {
                if let Some(audio_mixer) = state.audio_mixer() {
                    audio_mixer.handle_event(event);
                }
            }
            (state, event) => {
                error!(?state, ?event, "Wrong event type for input")
            }
        }
    }

    fn media(&mut self) -> Option<&mut InputMediaStatsState> {
        match self {
            InputStatsState::Rtp(state) => Some(&mut state.media),
            InputStatsState::Whip(state) => Some(&mut state.media),
            InputStatsState::Whep(state) => Some(&mut state.media),
            InputStatsState::Hls(state) => Some(&mut state.media),
            InputStatsState::Rtmp(state) => Some(&mut state.media),
            InputStatsState::MoqClient(state) => Some(&mut state.media),
            InputStatsState::Mp4(state) => Some(&mut state.media),
            InputStatsState::V4l2(_)
            | InputStatsState::DeckLink(_)
            | InputStatsState::RawData(_) => None,
        }
    }

    fn audio_mixer(&mut self) -> Option<&mut AudioMixerInputStatsState> {
        match self {
            InputStatsState::Rtp(state) => Some(&mut state.audio_mixer),
            InputStatsState::Whip(state) => Some(&mut state.audio_mixer),
            InputStatsState::Whep(state) => Some(&mut state.audio_mixer),
            InputStatsState::Hls(state) => Some(&mut state.audio_mixer),
            InputStatsState::Rtmp(state) => Some(&mut state.audio_mixer),
            InputStatsState::MoqClient(state) => Some(&mut state.audio_mixer),
            InputStatsState::Mp4(state) => Some(&mut state.audio_mixer),
            InputStatsState::V4l2(_)
            | InputStatsState::DeckLink(_)
            | InputStatsState::RawData(_) => None,
        }
    }

    pub fn report(&mut self) -> InputStatsReport {
        match self {
            InputStatsState::Rtp(state) => InputStatsReport::Rtp(state.report()),
            InputStatsState::Whip(state) => InputStatsReport::Whip(state.report()),
            InputStatsState::Whep(state) => InputStatsReport::Whep(state.report()),
            InputStatsState::Hls(state) => InputStatsReport::Hls(state.report()),
            InputStatsState::Rtmp(state) => InputStatsReport::Rtmp(state.report()),
            InputStatsState::MoqClient(state) => InputStatsReport::MoqClient(state.report()),
            InputStatsState::Mp4(state) => InputStatsReport::Mp4(state.report()),
            InputStatsState::V4l2(state) => InputStatsReport::V4l2(state.report()),
            InputStatsState::DeckLink(state) => InputStatsReport::DeckLink(state.report()),
            InputStatsState::RawData(state) => InputStatsReport::RawData(state.report()),
        }
    }
}
