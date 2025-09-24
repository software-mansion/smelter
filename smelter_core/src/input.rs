use std::{sync::Arc, time::Duration};

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct RegisterInputOptions {
    pub input_options: ProtocolInputOptions,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone)]
pub enum ProtocolInputOptions {
    Rtp(RtpInputOptions),
    Mp4(Mp4InputOptions),
    Hls(HlsInputOptions),
    Whip(WhipInputOptions),
    Whep(WhepInputOptions),
    #[cfg(feature = "decklink")]
    DeckLink(DeckLinkInputOptions),
}

#[derive(Debug, Clone, Copy)]
pub struct QueueInputOptions {
    pub required: bool,
    /// Relative offset this input stream should have to the clock that
    /// starts when pipeline is started.
    pub offset: Option<Duration>,
}

pub enum InputInitInfo {
    Rtp {
        port: Option<Port>,
    },
    Mp4 {
        video_duration: Option<Duration>,
        audio_duration: Option<Duration>,
    },
    Whip {
        bearer_token: Arc<str>,
    },
    Other,
}

#[derive(Debug)]
pub struct InputInfo {
    pub protocol: InputProtocolKind,
}

#[derive(Debug, Clone, Copy)]
pub enum InputProtocolKind {
    Rtp,
    Mp4,
    Hls,
    Whip,
    Whep,
    DeckLink,
    RawDataChannel,
}
