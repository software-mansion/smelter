use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr},
    thread,
    time::Duration,
};

use crate::common::CommunicationProtocol;
use anyhow::{Context, Result};
use bytes::{Bytes, BytesMut};
use crossbeam_channel::Receiver;
use tracing::error;
use webrtc::{rtcp, rtp, util::Unmarshal};

pub struct OutputReceiver {
    receiver: Receiver<Bytes>,
}

impl OutputReceiver {
    pub fn start(port: u16, protocol: CommunicationProtocol) -> Result<Self> {
        let mut socket = Self::setup_socket(port, &protocol)?;
        let mut output_dump = BytesMut::new();
        let (dump_sender, dump_receiver) = crossbeam_channel::bounded(1);

        thread::spawn(move || loop {
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
