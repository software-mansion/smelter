use bytes::Bytes;

use crate::{
    RtmpMessageSerializeError,
    amf0::encode_avmplus_values,
    message::{
        CONTROL_MESSAGE_STREAM_ID, MAIN_CHUNK_STREAM_ID, RESERVED_CHUNK_STREAM_ID, RtmpMessage,
        event::event_into_raw,
    },
    protocol::{MessageType, RawMessage},
};

impl RtmpMessage {
    pub fn into_raw(self) -> Result<RawMessage, RtmpMessageSerializeError> {
        let result = match self {
            RtmpMessage::WindowAckSize { window_size } => RawMessage {
                msg_type: MessageType::WindowAckSize.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&window_size.to_be_bytes()[..]),
            },
            RtmpMessage::Acknowledgement { bytes_received } => RawMessage {
                msg_type: MessageType::Acknowledgement.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&bytes_received.to_be_bytes()[..]),
            },
            RtmpMessage::SetPeerBandwidth {
                bandwidth,
                limit_type,
            } => RawMessage {
                msg_type: MessageType::SetPeerBandwidth.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::from([&bandwidth.to_be_bytes()[..], &[limit_type]].concat()),
            },
            RtmpMessage::UserControl(msg) => RawMessage {
                msg_type: MessageType::UserControl.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: msg.into_raw(),
            },
            RtmpMessage::CommandMessageAmf3 { values, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf3.into_raw(),
                stream_id,
                chunk_stream_id: MAIN_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: encode_avmplus_values(&values)?,
            },
            RtmpMessage::SetChunkSize { chunk_size } => RawMessage {
                msg_type: MessageType::SetChunkSize.into_raw(),
                stream_id: 0,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&chunk_size.to_be_bytes()[..]),
            },
            RtmpMessage::Event { event, stream_id } => event_into_raw(event, stream_id)?,
            RtmpMessage::CommandMessage { msg, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf0.into_raw(),
                stream_id,
                chunk_stream_id: MAIN_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: msg.into_amf0_bytes()?,
            },
        };
        Ok(result)
    }
}
