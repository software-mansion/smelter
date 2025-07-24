mod channel;
mod hls;
mod mp4;
mod rtmp;
mod rtp;
mod webrtc;

pub use channel::*;
pub use hls::*;
pub use mp4::*;
pub use rtmp::*;
pub use rtp::*;
pub use webrtc::*;

#[cfg(feature = "decklink")]
mod decklink;
#[cfg(feature = "decklink")]
pub use decklink::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortOrRange {
    Exact(u16),
    Range((u16, u16)),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(pub u16);
