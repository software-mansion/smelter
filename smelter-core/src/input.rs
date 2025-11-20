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
    #[cfg(target_os = "linux")]
    V4L2(V4L2InputOptions),
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
    V4L2,
    DeckLink,
    RawDataChannel,
}

#[derive(Debug, Clone, Copy)]
pub enum InputBufferOptions {
    /// No buffering, should only be used with required or if offset
    /// guarantees a enough time to deliver media to the queue.
    None,
    /// Fixed buffer, default to pipeline default (80ms).
    Const(Option<Duration>),
    /// Buffer that can increase and decrease to minimize latency.
    /// Desired buffer size is set based on pipeline default (80ms).
    LatencyOptimized,
    /// Buffer that can increase if packets are not delivered on time.
    /// It will never decrease even network conditions improve.
    Adaptive,
}
