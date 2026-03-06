use std::{
    collections::HashMap,
    net::TcpStream,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::channel},
};

use tracing::{debug, warn};

use crate::{
    RtmpServerConnectionError, RtmpStreamError, TlsConfig,
    amf0::Amf0Value,
    message::{
        CONTROL_MESSAGE_STREAM_ID, CommandMessage, CommandMessageOk, RtmpMessage,
        UserControlMessage,
    },
    protocol::{
        byte_stream::RtmpByteStream, handshake::Handshake, message_stream::RtmpMessageStream,
    },
    server::{
        OnConnectionCallback, RtmpConnection,
        negotiation::{NegotiationProgress, NegotiationResult, PEER_BANDWIDTH, WINDOW_ACK_SIZE},
    },
    transport::RtmpTransport,
};

/// For server we can pick this number for client it would be based on value
/// that came as _result for createStream
pub(crate) const PUBLISHED_MESSAGE_STREAM_ID: u32 = 1;

pub(crate) fn handle_connection(
    socket: TcpStream,
    on_connection: Arc<Mutex<OnConnectionCallback>>,
    tls_config: Option<TlsConfig>,
) -> Result<(), RtmpServerConnectionError> {
    let should_close = Arc::new(AtomicBool::new(false));
    let transport = match &tls_config {
        Some(tls_config) => RtmpTransport::tls_server_stream(socket, tls_config)?,
        None => RtmpTransport::tcp_server_stream(socket),
    };
    let mut stream = RtmpByteStream::new(transport, should_close);

    Handshake::perform_as_server(&mut stream)?;
    debug!("Handshake complete");

    let mut state = RtmpServerConnectionState {
        stream: RtmpMessageStream::new(stream),
        window_size: None,
        last_ack: 0,
    };

    let NegotiationResult { app, stream_key } = state.negotiate_connection()?;
    debug!(?app, ?stream_key, "Negotiation complete");

    let (sender, receiver) = channel();
    {
        let mut cb = on_connection.lock().unwrap();
        cb(RtmpConnection {
            app,
            stream_key,
            receiver, // TODO instead of returning a receiver, return custom iterator that exposes buffer details
        });
    }

    loop {
        let msg = state.next_msg()?;

        match msg {
            RtmpMessage::Event { event, .. } => {
                if sender.send(event).is_err() {
                    return Ok(());
                }
            }
            msg => state.default_msg_handler(msg)?,
        };
    }
}

struct RtmpServerConnectionState {
    stream: RtmpMessageStream,

    /// window size for data incoming from the client
    window_size: Option<u64>,
    /// last ack sent to client
    last_ack: u64,
}

impl RtmpServerConnectionState {
    fn next_msg(&mut self) -> Result<RtmpMessage, RtmpServerConnectionError> {
        loop {
            match self.stream.read_msg() {
                // should catch unknown messages or parsing error that
                // do not break stream continuity
                Err(err) if !err.is_critical() => {
                    warn!(?err);
                    continue;
                }
                Ok(msg) => return Ok(msg),
                Err(err) => return Err(err.into()),
            }
        }
    }

    fn negotiate_connection(&mut self) -> Result<NegotiationResult, RtmpServerConnectionError> {
        let mut state = NegotiationProgress::WaitingForConnect;

        loop {
            let msg = self.next_msg()?;

            if let Some((transaction_id, app)) = state.try_match_connect(&msg) {
                state = NegotiationProgress::WaitingForCreateStream { app };
                self.on_connect(transaction_id)?;
                continue;
            }

            if let Some((transaction_id, app)) = state.try_match_create_stream(&msg) {
                state = NegotiationProgress::WaitingForPublish { app };

                self.stream.write_msg(RtmpMessage::CommandMessage {
                    msg: CommandMessageOk {
                        transaction_id,
                        command_object: Amf0Value::Null,
                        response: Amf0Value::Number(PUBLISHED_MESSAGE_STREAM_ID as f64),
                    }
                    .into(),
                    stream_id: CONTROL_MESSAGE_STREAM_ID,
                })?;

                self.stream.write_msg(
                    UserControlMessage::StreamBegin {
                        stream_id: PUBLISHED_MESSAGE_STREAM_ID,
                    }
                    .into(),
                )?;
                continue;
            }

            if let Some(result) = state.try_match_publish(&msg) {
                let status_info = HashMap::from_iter(
                    [
                        ("level", "status".into()),
                        ("code", "NetStream.Publish.Start".into()),
                        ("description", "Publishing stream".into()),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.into(), v)),
                );

                self.stream.write_msg(RtmpMessage::CommandMessage {
                    msg: CommandMessage::OnStatus(Amf0Value::Object(status_info)),
                    stream_id: PUBLISHED_MESSAGE_STREAM_ID,
                })?;
                return Ok(result);
            }

            self.default_msg_handler(msg)?
        }
    }

    fn on_connect(&mut self, transaction_id: u32) -> Result<(), RtmpServerConnectionError> {
        self.stream.write_msg(RtmpMessage::WindowAckSize {
            window_size: WINDOW_ACK_SIZE,
        })?;
        self.stream.write_msg(RtmpMessage::SetPeerBandwidth {
            bandwidth: PEER_BANDWIDTH,
            limit_type: 0, // 0 - Hard, 1 - Soft, 2 - Dynamic
        })?;

        self.stream
            .write_msg(UserControlMessage::StreamBegin { stream_id: 0 }.into())?;

        // _result - connect response
        let props = HashMap::from_iter(
            [
                ("fmsVer", "FMS/3,0,1,123".into()),
                ("capabilities", Amf0Value::Number(31.0)),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v)),
        );
        let info = HashMap::from_iter(
            [
                ("level", "status".into()),
                ("code", "NetConnection.Connect.Success".into()),
                ("description", "Connection succeeded".into()),
                ("objectEncoding", Amf0Value::Number(0 as f64)), // AMF0 encoding
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v)),
        );
        self.stream.write_msg(RtmpMessage::CommandMessage {
            msg: CommandMessageOk {
                transaction_id,
                command_object: Amf0Value::Object(props),
                response: Amf0Value::Object(info),
            }
            .into(),
            stream_id: CONTROL_MESSAGE_STREAM_ID,
        })?;
        Ok(())
    }

    /// Message handler for messages not related to life cycle
    fn default_msg_handler(&mut self, msg: RtmpMessage) -> Result<(), RtmpStreamError> {
        match msg {
            RtmpMessage::SetChunkSize { chunk_size } => {
                self.stream.set_reader_chunk_size(chunk_size as usize);
            }
            RtmpMessage::WindowAckSize { window_size } => {
                self.window_size = Some(window_size as u64);
            }
            RtmpMessage::Acknowledgement { .. } => {
                // Server does not send much data, so receiving ACK will
                // be very rare
            }
            RtmpMessage::SetPeerBandwidth { bandwidth, .. } => {
                // It configures how often client will be sending ACKs,
                // it is different that self.window_size
                self.stream.write_msg(RtmpMessage::WindowAckSize {
                    window_size: bandwidth,
                })?;
            }
            RtmpMessage::UserControl(UserControlMessage::PingRequest { timestamp }) => {
                let msg = UserControlMessage::PingResponse { timestamp };
                self.stream.write_msg(msg.into())?;
            }
            _ => {
                debug!(?msg, "Unhandled message")
            }
        }

        self.maybe_send_ack()?;

        Ok(())
    }

    fn maybe_send_ack(&mut self) -> Result<(), RtmpStreamError> {
        let Some(window_size) = self.window_size else {
            return Ok(());
        };
        let bytes_received = self.stream.bytes_read();
        if bytes_received.saturating_sub(self.last_ack) > window_size / 2 {
            self.stream.write_msg(RtmpMessage::Acknowledgement {
                bytes_received: (bytes_received % (u32::MAX as u64 + 1)) as u32,
            })?;
            self.last_ack = bytes_received;
        }
        Ok(())
    }
}
