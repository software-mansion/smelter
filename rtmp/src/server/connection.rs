use std::{
    net::TcpStream,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::channel},
};

use tracing::{debug, trace, warn};

use crate::{
    RtmpServerConnectionError, RtmpStreamError,
    message::RtmpMessage,
    protocol::{
        handshake::Handshake, message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter,
        socket::NonBlockingSocket,
    },
    server::{OnConnectionCallback, RtmpConnection, negotiation::negotiate_rtmp_session},
};

pub(crate) fn handle_connection(
    socket: TcpStream,
    on_connection: Arc<Mutex<OnConnectionCallback>>,
) -> Result<(), RtmpServerConnectionError> {
    let should_close = Arc::new(AtomicBool::new(false));
    let (mut reader, mut writer) = NonBlockingSocket::from_tcp(socket, should_close).split(); // TODO: support TLS on input

    Handshake::perform_as_server(&mut reader, &mut writer)?;
    debug!("Handshake complete");

    let mut writer = RtmpMessageWriter::new(writer);
    let mut reader = RtmpMessageReader::new(reader);

    let (app, stream_key) = negotiate_rtmp_session(&mut reader, &mut writer)?;

    debug!(?app, ?stream_key, "Negotiation complete");

    let (sender, receiver) = channel();

    let connection_ctx = RtmpConnection {
        app: app.into(),
        stream_key: stream_key.into(),
        receiver, // TODO instead of returning a receiver, return custom iterator that exposes buffer details
    };

    {
        let mut cb = on_connection.lock().unwrap();
        cb(connection_ctx);
    }

    loop {
        let msg = match reader.next() {
            Ok(msg) => msg,
            Err(RtmpStreamError::ParseMessage(err)) => {
                warn!(?err, "Received unknown msg");
                continue;
            }
            Err(err) => return Err(err.into()),
        };
        trace!(?msg, "RTMP message received");

        let event = match msg {
            RtmpMessage::Event { event, .. } => event,
            _ => continue, // TODO: maybe handle
        };

        if sender.send(event).is_err() {
            return Ok(());
        }
    }
}
