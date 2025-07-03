use std::{
    collections::VecDeque,
    io::Read,
    net::TcpStream,
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::Duration,
};

use bytes::BytesMut;
use compositor_render::{error::ErrorStack, InputId};
use crossbeam_channel::{bounded, Receiver, Sender};
use log::error;
use tracing::{debug, info, span, trace, Level};

use crate::pipeline::{rtp::bind_to_requested_port, Port};

use super::{RtpReceiverError, RtpReceiverOptions};

pub(super) fn start_tcp_server_thread(
    input_id: &InputId,
    opts: &RtpReceiverOptions,
    should_close: Arc<AtomicBool>,
) -> Result<(Port, Receiver<bytes::Bytes>), RtpReceiverError> {
    let (packets_tx, packets_rx) = bounded(1000);
    let input_id = input_id.clone();
    info!(?input_id, "Starting tcp socket");

    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .map_err(RtpReceiverError::SocketOptions)?;

    let port = bind_to_requested_port(opts.port, &socket)?;

    socket.listen(1).map_err(RtpReceiverError::SocketBind)?;

    let socket = std::net::TcpListener::from(socket);

    thread::Builder::new()
        .name(format!("RTP TCP server receiver {input_id}"))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "RTP TCP server",
                input_id = input_id.to_string()
            )
            .entered();
            run_tcp_server_thread(socket, packets_tx, should_close);
            debug!("Closing RTP receiver thread (TCP server).");
        })
        .unwrap();

    Ok((port, packets_rx))
}

fn run_tcp_server_thread(
    socket: std::net::TcpListener,
    packets_tx: Sender<bytes::Bytes>,
    should_close: Arc<AtomicBool>,
) {
    // make accept non blocking so we have a chance to handle should_close value
    socket
        .set_nonblocking(true)
        .expect("Cannot set non-blocking");

    let mut connected_socket = None;
    while !should_close.load(std::sync::atomic::Ordering::Relaxed) && connected_socket.is_none() {
        // accept only one connection at the time
        let Ok((socket, _)) = socket.accept() else {
            thread::sleep(Duration::from_millis(50));
            continue;
        };
        connected_socket = Some(socket);
    }

    let socket = match connected_socket {
        Some(socket) => TcpReadPacketStream::new(socket, should_close.clone()),
        None => {
            return;
        }
    };

    for packet in socket {
        trace!(size_bytes = packet.len(), "Received RTP packet");
        if packets_tx.send(packet).is_err() {
            debug!("Failed to send raw RTP packet from TCP server element. Channel closed.");
            return;
        }
    }
}

struct TcpReadPacketStream {
    socket: TcpStream,
    buf: VecDeque<u8>,
    read_buf: Vec<u8>,
    should_close: Arc<AtomicBool>,
}

impl TcpReadPacketStream {
    fn new(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        socket
            .set_nonblocking(false)
            .expect("Cannot set blocking tcp input stream");
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("Cannot set read timeout");
        Self {
            socket,
            buf: VecDeque::new(),
            read_buf: vec![0; 65536],
            should_close,
        }
    }

    fn read_until_buffer_size(&mut self, buf_size: usize) -> Option<()> {
        loop {
            if self.buf.len() >= buf_size {
                return Some(());
            }
            match self.socket.read(&mut self.read_buf) {
                Ok(0) => return None,
                Ok(read_bytes) => {
                    self.buf.extend(self.read_buf[0..read_bytes].iter());
                }
                Err(err) => {
                    let should_close = self.should_close.load(std::sync::atomic::Ordering::Relaxed);
                    match err.kind() {
                        std::io::ErrorKind::WouldBlock if !should_close => {
                            continue;
                        }
                        std::io::ErrorKind::WouldBlock => return None,
                        _ => {
                            error!(
                                "Error while reading from TCP socket: {}",
                                ErrorStack::new(&err).into_string()
                            );
                            return None;
                        }
                    }
                }
            };
        }
    }
}

impl Iterator for TcpReadPacketStream {
    type Item = bytes::Bytes;

    fn next(&mut self) -> Option<Self::Item> {
        self.read_until_buffer_size(2)?;

        let mut len_bytes = [0u8; 2];
        self.buf.read_exact(&mut len_bytes).unwrap();
        let len = u16::from_be_bytes(len_bytes) as usize;

        self.read_until_buffer_size(len)?;
        let mut packet = BytesMut::zeroed(len);
        self.buf.read_exact(&mut packet[..]).unwrap();
        Some(packet.freeze())
    }
}
