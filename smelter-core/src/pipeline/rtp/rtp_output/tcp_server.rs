use std::{
    io::{self, Write},
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::Duration,
};

use tracing::{debug, trace, warn};

use crate::{
    error::OutputInitError,
    pipeline::{
        rtp::util::{bind_to_requested_port, BindToPortError},
        Port,
    },
    protocols::PortOrRange,
};

use super::RtpBinaryPacketStream;

pub(super) fn tcp_socket(port: PortOrRange) -> Result<(socket2::Socket, Port), OutputInitError> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .map_err(OutputInitError::SocketError)?;

    let port = bind_to_requested_port(port, &socket)?;

    socket.listen(1).map_err(OutputInitError::SocketError)?;
    Ok((socket, port))
}

pub(super) fn run_tcp_sender_thread(
    socket: socket2::Socket,
    should_close: Arc<AtomicBool>,
    packet_stream: RtpBinaryPacketStream,
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

    let mut socket = match connected_socket {
        Some(socket) => TcpWritePacketStream::new(socket, should_close.clone()),
        None => return,
    };

    for packet in packet_stream.flatten() {
        trace!(size_bytes = packet.len(), "Send RTP TCP packet.");
        if let Err(err) = socket.write_packet(packet) {
            if err.kind() == io::ErrorKind::WouldBlock {
                // this means that should_close is true
                return;
            }
            debug!("Failed to send RTP packet: {err}");
            continue;
        }
    }
    if let Err(err) = socket.socket.flush() {
        warn!(%err, "Failed to flush rest of the TCP buffer.");
    }
}

impl From<BindToPortError> for OutputInitError {
    fn from(value: BindToPortError) -> Self {
        match value {
            BindToPortError::SocketBind(err) => OutputInitError::SocketError(err),
            BindToPortError::PortAlreadyInUse(port) => OutputInitError::PortAlreadyInUse(port),
            BindToPortError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            } => OutputInitError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            },
        }
    }
}

struct TcpWritePacketStream {
    socket: socket2::Socket,
    should_close: Arc<AtomicBool>,
}

impl TcpWritePacketStream {
    fn new(socket: socket2::Socket, should_close: Arc<AtomicBool>) -> Self {
        // Timeout to make sure we are not left with unregistered
        // connections that are still maintained by a client side.
        socket
            .set_nonblocking(false)
            .expect("Cannot set blocking tcp output stream");
        socket
            .set_write_timeout(Some(Duration::from_secs(30)))
            .expect("Cannot set write timeout");
        Self {
            socket,
            should_close,
        }
    }

    fn write_packet(&mut self, data: bytes::Bytes) -> io::Result<()> {
        self.write_bytes(&u16::to_be_bytes(data.len() as u16))?;
        self.write_bytes(&data[..])?;
        io::Result::Ok(())
    }

    fn write_bytes(&mut self, data: &[u8]) -> io::Result<()> {
        let mut written_bytes = 0;
        loop {
            if written_bytes >= data.len() {
                return Ok(());
            }
            match self.socket.write(&data[written_bytes..]) {
                Ok(bytes) => {
                    written_bytes += bytes;
                }
                Err(err) => {
                    let should_close = self.should_close.load(std::sync::atomic::Ordering::Relaxed);
                    match err.kind() {
                        std::io::ErrorKind::WouldBlock if !should_close => {
                            continue;
                        }
                        _ => return io::Result::Err(err),
                    }
                }
            };
        }
    }
}
