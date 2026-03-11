use std::collections::HashMap;

use tracing::{debug, warn};

use crate::{
    RtmpConnectionError, RtmpEvent, RtmpMessageParseError,
    amf0::AmfValue,
    error::RtmpStreamError,
    message::{
        CONTROL_MESSAGE_STREAM_ID, CommandMessage, CommandMessageConnectSuccess,
        CommandMessageCreateStreamSuccess, CommandMessageResultExt, RtmpMessage,
        UserControlMessage,
    },
    protocol::{
        byte_stream::RtmpByteStream, handshake::Handshake, message_stream::RtmpMessageStream,
    },
    transport::RtmpTransport,
    utils::ShutdownCondition,
};

const CONNECT_TRANSACTION_ID: u32 = 1;
const CREATE_STREAM_TRANSACTION_ID: u32 = 2;

pub struct RtmpClientConfig {
    pub host: String,
    pub port: u16,
    pub app: String,
    pub stream_key: String,
    pub use_tls: bool,
}

pub struct RtmpClient {
    state: RtmpClientState,
    stream_id: u32,
    shutdown_condition: ShutdownCondition,
}

struct RtmpClientState {
    stream: RtmpMessageStream,

    /// window size for data incoming from the server
    window_size: Option<u64>,
    /// last ack sent to client
    last_ack: u64,
}

impl RtmpClient {
    pub fn connect(config: RtmpClientConfig) -> Result<Self, RtmpConnectionError> {
        let shutdown_condition = ShutdownCondition::default();

        let transport = if config.use_tls {
            RtmpTransport::tls_client(&config.host, config.port)?
        } else {
            RtmpTransport::tcp_client(&config.host, config.port)?
        };
        let mut socket = RtmpByteStream::new(transport, shutdown_condition.clone());

        Handshake::perform_as_client(&mut socket)?;
        debug!("Handshake complete");

        let mut state = RtmpClientState {
            stream: RtmpMessageStream::new(socket),
            window_size: None,
            last_ack: 0,
        };

        let stream_id = state.negotiate_connection(&config.app, &config.stream_key)?;
        debug!("Negotiation complete");

        Ok(Self {
            state,
            stream_id,
            shutdown_condition,
        })
    }

    pub fn send<T>(&mut self, event: T) -> Result<(), RtmpStreamError>
    where
        RtmpEvent: From<T>,
    {
        let event = RtmpEvent::from(event);
        self.state.stream.write_msg(RtmpMessage::Event {
            event,
            stream_id: self.stream_id,
        })?;

        // try read any pending messages
        while let Some(msg) = self.state.stream.try_read_msg()? {
            self.state.default_msg_handler(msg)?;
        }
        Ok(())
    }
}

impl Drop for RtmpClient {
    fn drop(&mut self) {
        self.shutdown_condition.mark_for_shutdown();
    }
}

impl RtmpClientState {
    fn negotiate_connection(
        &mut self,
        app: &str,
        stream_key: &str,
    ) -> Result<u32, RtmpConnectionError> {
        let mut state = NegotiationProgress::WaitingForConnectResult;
        send_connect(&mut self.stream, app)?;

        loop {
            let msg = match self.stream.read_msg() {
                Ok(msg) => msg,
                Err(RtmpStreamError::ParseMessage(err)) => {
                    warn!(%err, "Received unknown msg");
                    continue;
                }
                Err(err) => return Err(err.into()),
            };

            if let Some(_response) = state.try_match_connect_response(&msg)? {
                state = NegotiationProgress::WaitingForCreateStreamResult;
                send_create_stream(&mut self.stream)?;
                continue;
            }

            if let Some(response) = state.try_match_create_stream_response(&msg)? {
                state = NegotiationProgress::WaitingForOnStatus {
                    stream_id: response.stream_id,
                };
                send_publish(&mut self.stream, stream_key, response.stream_id)?;

                // should be after StreamBegin but e.g. YouTube does not send it
                self.stream
                    .write_msg(RtmpMessage::SetChunkSize { chunk_size: 4096 })?;
                self.stream.set_writer_chunk_size(4096);
                continue;
            }

            if let Some((_on_status, stream_id)) = state.try_match_on_status(&msg) {
                return Ok(stream_id);
            }

            self.default_msg_handler(msg)?
        }
    }

    fn default_msg_handler(&mut self, msg: RtmpMessage) -> Result<(), RtmpStreamError> {
        match msg {
            RtmpMessage::SetChunkSize { chunk_size } => {
                self.stream.set_reader_chunk_size(chunk_size as usize);
            }
            RtmpMessage::WindowAckSize { window_size } => {
                // Client does not receive much data, so sending ACKs
                // will be very rare.
                self.window_size = Some(window_size as u64);
            }
            RtmpMessage::Acknowledgement { .. } => {
                // TODO: throttle sending based on acks
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

        // not sure if it is necessary for client
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

/// -> - from client to server
/// <- - from server to client
///
/// indented steps are not reliable, assume that they can happen at different point or
/// not at all
enum NegotiationProgress {
    /// -> connect
    ///     <- Window Ack size
    ///     <- Set Peer Bandwidth
    ///     -> Window Ack Size
    ///     <- StreamBegin (with stream id 0)
    /// <- connect _result
    WaitingForConnectResult,

    /// -> createStream
    /// <- createStream _result
    WaitingForCreateStreamResult,

    /// -> publish
    ///     <- StreamBegin (with real stream id)
    ///     -> DataMessage (metadata)       TODO
    ///     -> SetChunkSize                 TODO
    /// <- onStatus
    WaitingForOnStatus { stream_id: u32 },
}

impl NegotiationProgress {
    fn try_match_connect_response(
        &self,
        msg: &RtmpMessage,
    ) -> Result<Option<CommandMessageConnectSuccess>, RtmpConnectionError> {
        let NegotiationProgress::WaitingForConnectResult = self else {
            return Ok(None);
        };

        let RtmpMessage::CommandMessage { msg, .. } = msg else {
            return Ok(None);
        };
        let CommandMessage::Result(result) = msg else {
            return Ok(None);
        };

        if result.transaction_id() != CONNECT_TRANSACTION_ID {
            return Ok(None);
        }

        match result {
            Ok(result) => {
                let connect_success = result
                    .to_connect_success()
                    .map_err(RtmpMessageParseError::CommandMessage)
                    .map_err(RtmpStreamError::ParseMessage)?;
                Ok(Some(connect_success))
            }
            Err(err) => Err(RtmpConnectionError::ErrorOnConnect(format!("{err:?}"))),
        }
    }

    fn try_match_create_stream_response(
        &self,
        msg: &RtmpMessage,
    ) -> Result<Option<CommandMessageCreateStreamSuccess>, RtmpConnectionError> {
        let NegotiationProgress::WaitingForCreateStreamResult = self else {
            return Ok(None);
        };

        let RtmpMessage::CommandMessage { msg, .. } = msg else {
            return Ok(None);
        };
        let CommandMessage::Result(result) = msg else {
            return Ok(None);
        };

        if result.transaction_id() != CREATE_STREAM_TRANSACTION_ID {
            return Ok(None);
        }

        match result {
            Ok(result) => {
                let create_stream_success = result
                    .to_create_stream_success()
                    .map_err(RtmpMessageParseError::CommandMessage)
                    .map_err(RtmpStreamError::ParseMessage)?;
                Ok(Some(create_stream_success))
            }
            Err(err) => Err(RtmpConnectionError::ErrorOnCreateStream(format!("{err:?}"))),
        }
    }

    fn try_match_on_status(&self, msg: &RtmpMessage) -> Option<(AmfValue, u32)> {
        let NegotiationProgress::WaitingForOnStatus { stream_id } = self else {
            return None;
        };

        let RtmpMessage::CommandMessage {
            msg: CommandMessage::OnStatus(status),
            stream_id: on_status_stream_id,
        } = msg
        else {
            return None;
        };

        if on_status_stream_id != stream_id {
            return None;
        }
        Some((status.clone(), *stream_id))
    }
}

fn send_connect(stream: &mut RtmpMessageStream, app: &str) -> Result<(), RtmpConnectionError> {
    let props = HashMap::from_iter(
        [
            ("app", app.into()),
            ("flashVer", "FMS/3,0,1,123".into()),
            // True if proxy is being used
            ("fpad", AmfValue::Boolean(false)),
            // TODO: add config option
            ("audioCodecs", AmfValue::Number(0x0FFF as f64)), // all RTMP supported
            // TODO: add config option
            ("videoCodecs", AmfValue::Number(0x00FF as f64)), // all RTMP supported
            ("videoFunction", AmfValue::Number(0.0)),
            ("objectEncoding", AmfValue::Number(0.0)), // TODO: add amf3
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v)),
    );

    stream.write_msg(RtmpMessage::CommandMessage {
        msg: CommandMessage::Connect {
            transaction_id: CONNECT_TRANSACTION_ID,
            command_object: props,
            optional_args: None,
        },
        stream_id: CONTROL_MESSAGE_STREAM_ID,
    })?;
    Ok(())
}

fn send_create_stream(stream: &mut RtmpMessageStream) -> Result<(), RtmpConnectionError> {
    stream.write_msg(RtmpMessage::CommandMessage {
        msg: CommandMessage::CreateStream {
            transaction_id: CREATE_STREAM_TRANSACTION_ID,
            command_object: AmfValue::Null,
        },
        stream_id: CONTROL_MESSAGE_STREAM_ID,
    })?;
    Ok(())
}

fn send_publish(
    stream: &mut RtmpMessageStream,
    stream_key: &str,
    stream_id: u32,
) -> Result<(), RtmpConnectionError> {
    stream.write_msg(RtmpMessage::CommandMessage {
        msg: CommandMessage::Publish {
            stream_key: stream_key.to_string(),
            publishing_type: "live".to_string(),
        },
        stream_id,
    })?;
    Ok(())
}
