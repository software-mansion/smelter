mod amf0;
mod amf3;
mod client;
mod error;
mod events;
mod flv;
mod message;
mod protocol;
mod server;
mod transport;
mod utils;

pub use client::*;
pub use error::*;
pub use events::*;
pub use flv::*;
pub use server::*;

pub(crate) const VIDEO_FOURCC_LIST: [&str; 6] = ["av01", "vp09", "vp08", "hvc1", "vvc1", "avc1"];

/// Capability flags for `videoFourCcInfoMap` / `audioFourCcInfoMap` entries
/// in the E-RTMP connect handshake. See `enum FourCcInfoMask` in the spec.
pub(crate) const FOURCC_INFO_CAN_DECODE: u8 = 0x01;
pub(crate) const FOURCC_INFO_CAN_ENCODE: u8 = 0x02;
pub(crate) const FOURCC_INFO_CAN_FORWARD: u8 = 0x04;

/// Extended capability flags for the `capsEx` property in the E-RTMP connect
/// handshake. See `enum CapsExMask` in the spec.
pub(crate) const CAPS_EX_RECONNECT: u8 = 0x01;
pub(crate) const CAPS_EX_MODEX: u8 = 0x04;
pub(crate) const CAPS_EX_TIMESTAMP_NANO: u8 = 0x08;
