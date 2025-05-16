use std::time::Duration;

use crate::*;

#[derive(Debug, Clone)]
pub enum RegisterInputOptions {
    Rtp(RtpInputOptions),
    Mp4(Mp4Options),
    Whip(WhipOptions),
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLinkOptions),
}

#[derive(Debug, Clone)]
pub struct RtpInputOptions {
    pub port: RequestedPort,
    pub transport_protocol: TransportProtocol,
    pub video: Option<RtpInputVideoOptions>,
    pub audio: Option<RtpInputAudioOptions>,
    pub queue: queue::QueueInputOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpInputVideoOptions {
    pub options: decoder::VideoDecoderOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpInputAudioOptions {
    pub options: decoder::AudioDecoderOptions,
}

pub struct OutputAudioStream {
    pub options: encoder::EncoderOptions,
    pub payload_type: u8,
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
