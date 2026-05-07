use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crossbeam_channel::bounded;
use tracing::{debug, warn};

use crate::{
    RtmpEvent, RtmpServerConnectionError, RtmpStreamError,
    amf0::AmfValue,
    message::{
        AudioMessage, CONTROL_MESSAGE_STREAM_ID, CommandMessage, CommandMessageOk, DataMessage,
        RtmpMessage, UserControlMessage, VideoMessage,
    },
    protocol::{
        byte_stream::RtmpByteStream, handshake::Handshake, message_stream::RtmpMessageStream,
    },
    server::{
        instance::ServerConnectionCtx,
        negotiation::{NegotiationProgress, NegotiationResult, PEER_BANDWIDTH, WINDOW_ACK_SIZE},
    },
    transport::RtmpTransport,
};

use crate::{
    CAPS_EX_MODEX, CAPS_EX_RECONNECT, CAPS_EX_TIMESTAMP_NANO, FOURCC_INFO_CAN_DECODE,
    FOURCC_INFO_CAN_ENCODE, FOURCC_INFO_CAN_FORWARD,
};

use crate::VIDEO_FOURCC_LIST;

/// For server we can pick this number for client it would be based on value
/// that came as _result for createStream
const PUBLISHED_MESSAGE_STREAM_ID: u32 = 1;

pub(super) fn run_connection_thread(
    ctx: &Arc<Mutex<ServerConnectionCtx>>,
    transport: RtmpTransport,
) -> Result<(), RtmpServerConnectionError> {
    let shutdown_condition = ctx.lock().unwrap().shutdown_condition.clone();
    let mut stream = RtmpByteStream::new(transport, shutdown_condition);

    Handshake::perform_as_server(&mut stream)?;
    debug!("Handshake complete");

    let mut state = RtmpServerConnectionState {
        stream: RtmpMessageStream::new(stream),
    };

    let NegotiationResult { app, stream_key } = state.negotiate_connection()?;
    debug!(?app, ?stream_key, "Negotiation complete");

    let (sender, receiver) = bounded(1000);
    // Return connection to caller via on_connection callback
    ctx.lock()
        .unwrap()
        .send_connection(app, stream_key, receiver)?;

    loop {
        let msg = state.next_msg()?;

        let event = match msg {
            RtmpMessage::Audio { audio, .. } => match audio {
                AudioMessage::Data(data) => RtmpEvent::AudioData(data),
                AudioMessage::Config(config) => RtmpEvent::AudioConfig(config),
                AudioMessage::Unknown => continue,
            },
            RtmpMessage::Video { video, .. } => match video {
                VideoMessage::Data(data) => RtmpEvent::VideoData(data),
                VideoMessage::Config(config) => RtmpEvent::VideoConfig(config),
                VideoMessage::Unknown => continue,
            },
            RtmpMessage::DataMessage {
                data: DataMessage::OnMetaData(metadata),
                ..
            } => RtmpEvent::Metadata(metadata),
            RtmpMessage::CommandMessage {
                msg: CommandMessage::DeleteStream { .. },
                ..
            } => {
                return Ok(());
            }
            msg => {
                state.default_msg_handler(msg)?;
                continue;
            }
        };

        if sender.send(event).is_err() {
            debug!("Channel closed. Stopping connection.");
            return Ok(());
        }
    }
}

struct RtmpServerConnectionState {
    stream: RtmpMessageStream,
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
                        command_object: AmfValue::Null,
                        response: AmfValue::Number(PUBLISHED_MESSAGE_STREAM_ID as f64),
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
                    msg: CommandMessage::OnStatus(AmfValue::Object(status_info)),
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

        let video_fourcc_info_map = HashMap::from_iter([
            (
                "*".to_string(),
                AmfValue::Number(FOURCC_INFO_CAN_FORWARD as f64),
            ),
            (
                "avc1".to_string(),
                AmfValue::Number(
                    (FOURCC_INFO_CAN_DECODE | FOURCC_INFO_CAN_ENCODE | FOURCC_INFO_CAN_FORWARD)
                        as f64,
                ),
            ),
        ]);
        // _result - connect response
        let props = HashMap::from_iter(
            [
                ("fmsVer", "FMS/3,0,1,123".into()),
                ("capabilities", AmfValue::Number(31.0)),
                (
                    "fourCcList",
                    AmfValue::StrictArray(
                        VIDEO_FOURCC_LIST
                            .iter()
                            .map(|v| AmfValue::String((*v).to_string()))
                            .collect(),
                    ),
                ),
                (
                    "videoFourCcInfoMap",
                    AmfValue::Object(video_fourcc_info_map),
                ),
                // TODO: add audioFourCcInfoMap once enhanced audio tags are implemented.
                (
                    "capsEx",
                    AmfValue::Number(
                        (CAPS_EX_RECONNECT | CAPS_EX_MODEX | CAPS_EX_TIMESTAMP_NANO) as f64,
                    ),
                ),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v)),
        );
        let info = HashMap::from_iter(
            [
                ("level", "status".into()),
                ("code", "NetConnection.Connect.Success".into()),
                ("description", "Connection succeeded".into()),
                ("objectEncoding", AmfValue::Number(0 as f64)), // AMF0 encoding
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v)),
        );
        self.stream.write_msg(RtmpMessage::CommandMessage {
            msg: CommandMessageOk {
                transaction_id,
                command_object: AmfValue::Object(props),
                response: AmfValue::Object(info),
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
                self.stream.set_peer_window_ack_size(window_size as u64);
            }
            RtmpMessage::Acknowledgement { .. } => {
                // Server does not send much data, so receiving ACK will
                // be very rare
            }
            RtmpMessage::SetPeerBandwidth { bandwidth, .. } => {
                // Configures how often the peer will send ACKs to us — distinct
                // from our own incoming ack window tracked in session state.
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

        self.stream.maybe_send_ack()?;

        Ok(())
    }
}
