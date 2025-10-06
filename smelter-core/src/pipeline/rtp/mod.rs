use std::time::Duration;

pub(super) mod depayloader;
pub(super) mod dynamic_depayloader;
pub(super) mod payloader;

mod rtp_input;
mod rtp_output;
mod util;

pub(crate) use rtp_input::{
    RtpInput,
    rtcp_sync::{RtpNtpSyncPoint, RtpTimestampSync},
};
pub(crate) use rtp_output::RtpOutput;
use webrtc::rtp;

#[derive(Clone)]
pub struct RtpPacket {
    pub packet: rtp::packet::Packet,
    pub timestamp: Duration,
}

impl std::fmt::Debug for RtpPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let first_bytes = &self.packet.payload[0..usize::min(10, self.packet.payload.len())];
        f.debug_struct("RtpPacket")
            .field("header", &self.packet.header)
            .field("payload", &first_bytes)
            .field("timestamp", &self.timestamp)
            .finish()
    }
}
