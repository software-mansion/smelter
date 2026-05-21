mod amf0;
mod amf3;
mod client;
mod error;
mod events;
mod ex_capabilities;
mod flv;
mod message;
mod protocol;
mod server;
mod track;
mod transport;
mod utils;

pub use client::*;
pub use error::*;
pub use events::*;
pub use flv::AudioChannels;
pub use server::*;
pub use track::TrackId;

pub(crate) use ex_capabilities::*;
pub(crate) use flv::*;
pub(crate) use track::TrackKey;

pub(crate) const VIDEO_FOURCC_LIST: [&str; 3] = ["avc1", "vp09", "vp08"];
pub(crate) const AUDIO_FOURCC_LIST: [&str; 2] = ["mp4a", "Opus"];

/// Capability flags for `videoFourCcInfoMap` / `audioFourCcInfoMap` entries
/// in the E-RTMP connect handshake. See `enum FourCcInfoMask` in the spec.
pub(crate) const FOURCC_INFO_CAN_DECODE: u8 = 0x01;
pub(crate) const FOURCC_INFO_CAN_ENCODE: u8 = 0x02;
pub(crate) const FOURCC_INFO_CAN_FORWARD: u8 = 0x04;
