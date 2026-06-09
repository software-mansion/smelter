use std::{
    fs,
    io::Read,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    thread,
    time::Duration,
};

use crate::{common::CommunicationProtocol, pipeline_tests::start_server_msg_listener};
use anyhow::{Context, Result};
use bytes::{Bytes, BytesMut};
use crossbeam_channel::Receiver;
use tokio_tungstenite::tungstenite;
use tracing::{error, info};
use webrtc::{rtcp, rtp};
use webrtc_util::Unmarshal;

pub struct OutputReceiver {
    receiver: Receiver<Bytes>,
}

impl OutputReceiver {
    pub fn start(port: u16, protocol: CommunicationProtocol) -> Result<Self> {
        let mut socket = Self::setup_socket(port, &protocol)?;
        let mut output_dump = BytesMut::new();
        let (dump_sender, dump_receiver) = crossbeam_channel::bounded(1);

        thread::spawn(move || {
            loop {
                let packet = match Self::read_packet(&mut socket, &protocol) {
                    Ok(packet) => packet,
                    Err(err) => {
                        error!("Failed to read packet: {err:?}");
                        break;
                    }
                };

                match packet {
                    Packet::RtcpGoodbye => {
                        dump_sender.send(output_dump.freeze()).unwrap();
                        break;
                    }
                    Packet::Rtp(packet_bytes) => {
                        let packet_len = packet_bytes.len() as u16;
                        output_dump.extend(packet_len.to_be_bytes());
                        output_dump.extend(&packet_bytes);
                    }
                }
            }
        });

        Ok(Self {
            receiver: dump_receiver,
        })
    }

    pub fn wait_for_output(self) -> Result<Bytes> {
        self.receiver
            .recv_timeout(Duration::from_secs(120))
            .context("Failed to receive output dump")
    }

    fn setup_socket(port: u16, protocol: &CommunicationProtocol) -> Result<socket2::Socket> {
        let socket = match protocol {
            CommunicationProtocol::Udp => socket2::Socket::new(
                socket2::Domain::IPV4,
                socket2::Type::DGRAM,
                Some(socket2::Protocol::UDP),
            )?,
            CommunicationProtocol::Tcp => socket2::Socket::new(
                socket2::Domain::IPV4,
                socket2::Type::STREAM,
                Some(socket2::Protocol::TCP),
            )?,
        };

        match protocol {
            CommunicationProtocol::Udp => {
                socket.bind(&SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), port).into())?;
            }
            CommunicationProtocol::Tcp => {
                socket
                    .connect(&SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), port).into())?;
            }
        }

        Ok(socket)
    }

    fn read_packet(
        socket: &mut socket2::Socket,
        protocol: &CommunicationProtocol,
    ) -> Result<Packet> {
        match protocol {
            CommunicationProtocol::Udp => {
                let mut buffer = vec![0u8; u16::MAX as usize];
                let packet_len = socket.read(&mut buffer)?;

                unmarshal_packet(Bytes::from(buffer[..packet_len].to_vec()))
            }
            CommunicationProtocol::Tcp => {
                let mut packet_len_bytes = [0u8; 2];
                socket.read_exact(&mut packet_len_bytes)?;
                let packet_len = u16::from_be_bytes(packet_len_bytes) as usize;

                let mut buffer = BytesMut::zeroed(packet_len);
                socket.read_exact(&mut buffer[..])?;

                unmarshal_packet(buffer.freeze())
            }
        }
    }
}

/// Collects a muxed MP4 output written by the pipeline to a file.
///
/// Unlike [`OutputReceiver`], which reads RTP packets off a socket, the
/// MP4 muxer writes straight to disk and only finalises the container
/// (writing the `moov` trailer) once the output reaches end-of-stream.
/// We therefore watch the server's event websocket for the
/// `OUTPUT_DONE` event for our output id, then read the completed file.
pub struct Mp4OutputReceiver {
    path: PathBuf,
    output_id: String,
    msg_receiver: Receiver<tungstenite::Message>,
}

impl Mp4OutputReceiver {
    /// Start listening for output-completion events. Call this before
    /// `start`ing the pipeline so no event is missed.
    pub fn start(api_port: u16, output_id: &str, path: PathBuf) -> Self {
        let (sender, msg_receiver) = crossbeam_channel::unbounded();
        start_server_msg_listener(api_port, sender);
        Self {
            path,
            output_id: output_id.to_string(),
            msg_receiver,
        }
    }

    /// Block until the output finishes and the MP4 file is finalised,
    /// then read it back.
    pub fn wait_for_output(self) -> Result<Bytes> {
        let needle = format!(
            "\"type\":\"OUTPUT_DONE\",\"output_id\":\"{}\"",
            self.output_id
        );
        loop {
            let msg = self
                .msg_receiver
                .recv_timeout(Duration::from_secs(120))
                .context("Timed out waiting for OUTPUT_DONE event")?;
            if let tungstenite::Message::Text(text) = msg
                && text.contains(&needle)
            {
                info!("Received OUTPUT_DONE for output `{}`", self.output_id);
                break;
            }
        }
        let bytes = fs::read(&self.path)
            .with_context(|| format!("Failed to read MP4 output {}", self.path.display()))?;
        Ok(Bytes::from(bytes))
    }
}

fn unmarshal_packet(mut buffer: Bytes) -> Result<Packet> {
    let rtp_packet = rtp::packet::Packet::unmarshal(&mut buffer.clone())?;
    let packet = if rtp_packet.header.payload_type < 64 || rtp_packet.header.payload_type > 95 {
        Packet::Rtp(buffer)
    } else {
        rtcp::goodbye::Goodbye::unmarshal(&mut buffer).map(|_| Packet::RtcpGoodbye)?
    };

    Ok(packet)
}

#[derive(Debug, PartialEq, Eq)]
enum Packet {
    RtcpGoodbye,
    Rtp(Bytes),
}
