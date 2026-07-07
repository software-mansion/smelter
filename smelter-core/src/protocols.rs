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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortOrRange {
    Exact(u16),
    Range((u16, u16)),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(pub u16);
