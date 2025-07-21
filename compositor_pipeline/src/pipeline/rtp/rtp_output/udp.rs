use tracing::{debug, trace};

use crate::{error::OutputInitError, pipeline::Port};

use super::RtpBinaryPacketStream;

pub(super) fn udp_socket(ip: &str, port: Port) -> Result<(socket2::Socket, Port), OutputInitError> {
    let socket = std::net::UdpSocket::bind(std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::UNSPECIFIED,
        0,
    ))?;

    socket.connect((ip, port.0))?;
    Ok((socket.into(), port))
}

/// this assumes, that a "packet" contains data about a single frame (access unit)
pub(super) fn run_udp_sender_thread(socket: socket2::Socket, packet_stream: RtpBinaryPacketStream) {
    for packet in packet_stream.flatten() {
        trace!(size_bytes = packet.len(), "Send RTP UDP packet.");
        if let Err(err) = socket.send(&packet) {
            debug!("Failed to send packet: {err}");
        };
    }
}
