use tracing::error;

pub(super) mod audio_mixer;
pub(super) mod hls;
pub(super) mod media;
pub(super) mod moq_client;
pub(super) mod moq_server;
pub(super) mod mp4;
pub(super) mod rtmp;
pub(super) mod rtp;
pub(super) mod sync;
pub(super) mod whep;
pub(super) mod whip;

use crate::{
    InputProtocolKind,
    pipeline::utils::input_sync::TrackKind,
    stats::{
        input::audio_mixer::AudioMixerInputStatsState, input::hls::HlsInputState,
        input::media::InputMediaStatsState, input::moq_client::MoqClientInputState,
        input::moq_server::MoqServerInputState, input::mp4::Mp4InputState,
        input::rtmp::RtmpInputState, input::rtp::RtpInputState, input::whep::WhepInputState,
        input::whip::WhipInputState, input_reports::InputStatsReport,
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
    MoqServer(MoqServerInputState),
    MoqClient(MoqClientInputState),
    Mp4(Mp4InputState),
}

impl InputStatsState {
    /// `None` for inputs without stats.
    pub fn new(kind: InputProtocolKind) -> Option<Self> {
        let state = match kind {
            InputProtocolKind::Whip => InputStatsState::Whip(WhipInputState::new()),
            InputProtocolKind::Whep => InputStatsState::Whep(WhepInputState::new()),
            InputProtocolKind::Rtp => InputStatsState::Rtp(RtpInputState::new()),
            InputProtocolKind::Rtmp => InputStatsState::Rtmp(RtmpInputState::new()),
            InputProtocolKind::Mp4 => InputStatsState::Mp4(Mp4InputState::new()),
            InputProtocolKind::Hls => InputStatsState::Hls(HlsInputState::new()),
            InputProtocolKind::MoqServer => InputStatsState::MoqServer(MoqServerInputState::new()),
            InputProtocolKind::MoqClient => InputStatsState::MoqClient(MoqClientInputState::new()),
            InputProtocolKind::V4l2
            | InputProtocolKind::DeckLink
            | InputProtocolKind::RawDataChannel => return None,
        };
        Some(state)
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
            (InputStatsState::MoqServer(state), InputStatsEvent::Sync { track, event }) => {
                state.sync.handle_event(track, event);
            }
            (InputStatsState::MoqClient(state), InputStatsEvent::Sync { track, event }) => {
                state.sync.handle_event(track, event);
            }
            (state, InputStatsEvent::Media { track, event }) => {
                state.media().handle_event(track, event);
            }
            (state, InputStatsEvent::AudioMixer(event)) => {
                state.audio_mixer().handle_event(event);
            }
            (state, event) => {
                error!(?state, ?event, "Wrong event type for input")
            }
        }
    }

    fn media(&mut self) -> &mut InputMediaStatsState {
        match self {
            InputStatsState::Rtp(state) => &mut state.media,
            InputStatsState::Whip(state) => &mut state.media,
            InputStatsState::Whep(state) => &mut state.media,
            InputStatsState::Hls(state) => &mut state.media,
            InputStatsState::Rtmp(state) => &mut state.media,
            InputStatsState::MoqServer(state) => &mut state.media,
            InputStatsState::MoqClient(state) => &mut state.media,
            InputStatsState::Mp4(state) => &mut state.media,
        }
    }

    fn audio_mixer(&mut self) -> &mut AudioMixerInputStatsState {
        match self {
            InputStatsState::Rtp(state) => &mut state.audio_mixer,
            InputStatsState::Whip(state) => &mut state.audio_mixer,
            InputStatsState::Whep(state) => &mut state.audio_mixer,
            InputStatsState::Hls(state) => &mut state.audio_mixer,
            InputStatsState::Rtmp(state) => &mut state.audio_mixer,
            InputStatsState::MoqServer(state) => &mut state.audio_mixer,
            InputStatsState::MoqClient(state) => &mut state.audio_mixer,
            InputStatsState::Mp4(state) => &mut state.audio_mixer,
        }
    }

    pub fn report(&mut self) -> InputStatsReport {
        match self {
            InputStatsState::Rtp(state) => InputStatsReport::Rtp(state.report()),
            InputStatsState::Whip(state) => InputStatsReport::Whip(state.report()),
            InputStatsState::Whep(state) => InputStatsReport::Whep(state.report()),
            InputStatsState::Hls(state) => InputStatsReport::Hls(state.report()),
            InputStatsState::Rtmp(state) => InputStatsReport::Rtmp(state.report()),
            InputStatsState::MoqServer(state) => InputStatsReport::MoqServer(state.report()),
            InputStatsState::MoqClient(state) => InputStatsReport::MoqClient(state.report()),
            InputStatsState::Mp4(state) => InputStatsReport::Mp4(state.report()),
        }
    }
}
