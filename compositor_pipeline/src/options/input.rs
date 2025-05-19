use std::time::Duration;

use crate::*;

#[derive(Debug, Clone)]
pub struct RegisterInputOptions {
    protocol: ProtocolInputOptions,
    queue: QueueInputOptions,
} 

#[derive(Debug, Clone)]
pub enum ProtocolInputOptions {
    Rtp(RtpInputOptions),
    Mp4(Mp4InputOptions),
    Whip(WhipInputOptions),
    #[cfg(feature = "decklink")]
    DeckLink(DeckLinkInputOptions),
}

#[derive(Debug, Clone, Copy)]
pub struct QueueInputOptions {
    pub required: bool,
    /// Relative offset this input stream should have to the clock that
    /// starts when pipeline is started.
    pub offset: Option<Duration>,

    /// Duration of stream that should be buffered before stream is started.
    /// If you have both audio and video streams then make sure to use the same value
    /// to avoid desync.
    ///
    /// This value defines minimal latency on the queue, but if you set it to low and fail
    /// to deliver the input stream on time it can cause either black screen or flickering image.
    ///
    /// By default DEFAULT_BUFFER_DURATION will be used.
    pub buffer_duration: Option<Duration>,
}
