use std::time::Duration;

mod channel;
mod hls;
mod moq;
mod mp4;
mod rtmp;
mod rtp;
mod v4l2;
mod webrtc;

pub use channel::*;
pub use hls::*;
pub use moq::*;
pub use mp4::*;
pub use rtmp::*;
pub use rtp::*;
pub use v4l2::*;
pub use webrtc::*;

#[cfg(feature = "decklink")]
mod decklink;
#[cfg(feature = "decklink")]
pub use decklink::*;

/// Buffer a live input keeps between the live edge and playback. Values that
/// are not set are derived from the provided ones and protocol defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LiveInputBufferOptions {
    pub desired: Option<Duration>,
    pub min: Option<Duration>,
    pub max: Option<Duration>,
}

/// What a live input does when the stream can't be on time with the `max` buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveInputBufferMaxPolicy {
    /// The buffer grows past `max` as far as the stream needs, and nothing is lost.
    Extend,
    /// Playback is never more than `max` behind the live edge of the stream, measured on audio
    /// when the input has it. Audio that arrives too late is dropped, video is shown late.
    Enforce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortOrRange {
    Exact(u16),
    Range((u16, u16)),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(pub u16);
