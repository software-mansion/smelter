use std::{
    sync::{Arc, atomic::AtomicBool},
    thread,
};

use bytes::{Bytes, BytesMut};
use crossbeam_channel::{Receiver, Sender, unbounded};
use smelter_render::InputId;
use tracing::{Level, debug, span, warn};

use crate::pipeline::{Port, rtp::util::bind_to_requested_port};

use crate::prelude::*;

use super::{RtpInputError, RtpInputOptions};

pub(super) fn start_udp_reader_thread(
    input_ref: &Ref<InputId>,
    opts: &RtpInputOptions,
    should_close: Arc<AtomicBool>,
) -> Result<(Port, Receiver<bytes::Bytes>), RtpInputError> {
    let (packets_tx, packets_rx) = unbounded();

    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(RtpInputError::SocketOptions)?;

    match socket
        .set_recv_buffer_size(16 * 1024 * 1024)
        .map_err(RtpInputError::SocketOptions)
    {
        Ok(_) => {}
        Err(e) => {
            warn!(
                "Failed to set socket receive buffer size: {e} This may cause packet loss, especially on high-bitrate streams."
            );
        }
    }

    let port = bind_to_requested_port(opts.port, &socket)?;

    socket
        .set_read_timeout(Some(std::time::Duration::from_millis(50)))
        .map_err(RtpInputError::SocketOptions)?;

    let socket = std::net::UdpSocket::from(socket);

    let input_ref = input_ref.clone();
    thread::Builder::new()
        .name(format!("RTP UDP receiver {input_ref}"))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "RTP TCP server",
                input_id = input_ref.to_string()
            )
            .entered();
            run_udp_receiver_thread(socket, packets_tx, should_close);
            debug!("Closing RTP receiver thread (UDP).");
        })
        .unwrap();

    Ok((port, packets_rx))
}

fn run_udp_receiver_thread(
    socket: std::net::UdpSocket,
    packets_tx: Sender<Bytes>,
    should_close: Arc<AtomicBool>,
) {
    let mut buffer = BytesMut::zeroed(65536);

    loop {
        if should_close.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // This can be faster if we batched sending the packets through the channel
        let (received_bytes, _) = match socket.recv_from(&mut buffer) {
            Ok(n) => n,
            Err(e) => match e.kind() {
                std::io::ErrorKind::WouldBlock => continue,
                _ => {
                    tracing::error!("Error while receiving UDP packet: {}", e);
                    continue;
                }
            },
        };

        if packets_tx
            .send(Bytes::copy_from_slice(&buffer[..received_bytes]))
            .is_err()
        {
            debug!("Failed to send raw RTP packet from TCP server element. Channel closed.");
            return;
        }
    }
}
