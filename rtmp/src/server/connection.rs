use std::{
    net::TcpStream,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use tracing::{debug, trace};

use crate::{
    error::RtmpError,
    message::RtmpMessage,
    protocol::{
        handshake::Handshake, message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter,
    },
    server::{
        OnConnectionCallback, RtmpConnection, negotiation::negotiate_rtmp_session,
        rtmp_event_channel,
    },
};

pub(crate) fn handle_connection(
    mut stream: TcpStream,
    on_connection: Arc<Mutex<OnConnectionCallback>>,
) -> Result<(), RtmpError> {
    Handshake::perform_as_server(&mut stream)?;
    debug!("Handshake complete");

    let mut message_writer = RtmpMessageWriter::new(stream.try_clone()?);
    let mut message_reader = RtmpMessageReader::new(stream, Arc::new(AtomicBool::new(false)));

    let (app, stream_key) = negotiate_rtmp_session(&mut message_reader, &mut message_writer)?;

    debug!(?app, ?stream_key, "Negotiation complete");

    let (sender, receiver) = rtmp_event_channel();

    let connection_ctx = RtmpConnection {
        app: app.into(),
        stream_key: stream_key.into(),
        receiver,
    };

    {
        let mut cb = on_connection.lock().unwrap();
        cb(connection_ctx);
    }

    for msg in message_reader {
        let msg = msg?;
        trace!(?msg, "RTMP message received");

        let event = match msg {
            RtmpMessage::Event { event, .. } => event,
            _ => continue, // TODO: maybe handle
        };

        sender.send(event).map_err(|_| RtmpError::ChannelClosed)?;
    }
    Ok(())
}
