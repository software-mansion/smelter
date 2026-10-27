use std::{fmt, sync::Arc, time::Duration};

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub enum RegisterInputOptions {
    Rtp(RtpInputOptions),
    RtmpServer(RtmpServerInputOptions),
    MoqClient(MoqClientInputOptions),
    Mp4(Mp4InputOptions),
    Hls(HlsInputOptions),
    Whip(WhipInputOptions),
    Whep(WhepInputOptions),
    #[cfg(target_os = "linux")]
    V4l2(V4l2InputOptions),
    #[cfg(feature = "decklink")]
    DeckLink(DeckLinkInputOptions),
}

impl RegisterInputOptions {
    pub fn kind(&self) -> InputProtocolKind {
        match self {
            RegisterInputOptions::Rtp(_) => InputProtocolKind::Rtp,
            RegisterInputOptions::RtmpServer(_) => InputProtocolKind::Rtmp,
            RegisterInputOptions::MoqClient(_) => InputProtocolKind::MoqClient,
            RegisterInputOptions::Mp4(_) => InputProtocolKind::Mp4,
            RegisterInputOptions::Hls(_) => InputProtocolKind::Hls,
            RegisterInputOptions::Whip(_) => InputProtocolKind::Whip,
            RegisterInputOptions::Whep(_) => InputProtocolKind::Whep,
            #[cfg(target_os = "linux")]
            RegisterInputOptions::V4l2(_) => InputProtocolKind::V4l2,
            #[cfg(feature = "decklink")]
            RegisterInputOptions::DeckLink(_) => InputProtocolKind::DeckLink,
        }
    }
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
        endpoint_route: Arc<str>,
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
    Rtmp,
    MoqClient,
    Mp4,
    Hls,
    Whip,
    Whep,
    V4l2,
    DeckLink,
    RawDataChannel,
}

impl fmt::Display for InputProtocolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputProtocolKind::Rtp => write!(f, "rtp"),
            InputProtocolKind::Rtmp => write!(f, "rtmp"),
            InputProtocolKind::MoqClient => write!(f, "moq_client"),
            InputProtocolKind::Mp4 => write!(f, "mp4"),
            InputProtocolKind::Hls => write!(f, "hls"),
            InputProtocolKind::Whip => write!(f, "whip"),
            InputProtocolKind::Whep => write!(f, "whep"),
            InputProtocolKind::V4l2 => write!(f, "v4l2"),
            InputProtocolKind::DeckLink => write!(f, "decklink"),
            InputProtocolKind::RawDataChannel => write!(f, "raw_data_channel"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum InputBufferOptions {
    /// No buffering, should only be used with required or if offset
    /// guarantees a enough time to deliver media to the queue.
    None,
    /// Fixed buffer, default to pipeline default (80ms).
    Const { size: Duration },
    /// Buffer that can increase if packets are not delivered on time.
    /// It will never decrease even network conditions improve.
    Adaptive,
}
