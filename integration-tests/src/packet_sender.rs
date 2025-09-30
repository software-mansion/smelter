use std::{
    io::Write,
    net::{Ipv4Addr, SocketAddr},
    thread::{self, JoinHandle},
};

use crate::common::CommunicationProtocol;
use anyhow::Result;
use bytes::Bytes;

pub struct PacketSender {
    protocol: CommunicationProtocol,
    socket: socket2::Socket,
}

impl PacketSender {
    pub fn new(protocol: CommunicationProtocol, port: u16) -> Result<Self> {
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

        socket.connect(&SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), port).into())?;

        Ok(Self { protocol, socket })
    }

    pub fn send(&mut self, rtp_packets: &[u8]) -> Result<()> {
        match self.protocol {
            CommunicationProtocol::Udp => self.send_via_udp(rtp_packets),
            CommunicationProtocol::Tcp => self.send_via_tcp(rtp_packets),
        }
    }

    pub fn send_non_blocking(mut self, rtp_packets: Bytes) -> JoinHandle<()> {
        thread::spawn(move || {
            self.send(&rtp_packets).unwrap();
        })
    }

    fn send_via_udp(&mut self, rtp_packets: &[u8]) -> Result<()> {
        let mut sent_bytes = 0;
        while sent_bytes < rtp_packets.len() {
            let packet_len =
                u16::from_be_bytes([rtp_packets[sent_bytes], rtp_packets[sent_bytes + 1]]) as usize;
            sent_bytes += 2;

            let packet = &rtp_packets[sent_bytes..(sent_bytes + packet_len)];

            sent_bytes += packet_len;

            self.socket.write_all(packet)?;
        }

        Ok(())
    }

    fn send_via_tcp(&mut self, rtp_packets: &[u8]) -> Result<()> {
        self.socket.write_all(rtp_packets)?;
        Ok(())
    }
}
