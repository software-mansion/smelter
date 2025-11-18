use std::time::Duration;

pub(super) mod depayloader;
pub(super) mod payloader;

mod rtp_input;
mod rtp_output;
mod util;

pub(crate) use rtp_input::{
    RtpInput,
    jitter_buffer::{RtpJitterBuffer, RtpJitterBufferInitOptions},
};
pub(crate) use rtp_output::RtpOutput;

#[derive(Clone)]
pub struct RtpPacket {
    pub packet: webrtc::rtp::packet::Packet,
    pub timestamp: Duration,
}

#[derive(Debug, Clone)]
pub enum RtpInputEvent {
    Packet(RtpPacket),
    LostPacket,
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
