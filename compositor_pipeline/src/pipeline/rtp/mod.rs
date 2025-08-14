use std::time::Duration;

pub(super) mod depayloader;
pub(super) mod payloader;

mod rtp_input;
mod rtp_output;
mod util;

pub(crate) use rtp_input::{RtpInput, RtpTimestampSync};
pub(crate) use rtp_output::RtpOutput;

#[derive(Debug)]
pub struct RtpPacket {
    pub packet: rtp::packet::Packet,
    pub timestamp: Duration,
}
