use bytes::Bytes;

use crate::{
    SerializationError,
    amf0::{encode_amf0_values, encode_avmplus_values},
    message::{MAIN_CHUNK_STREAM_ID, RESERVED_CHUNK_STREAM_ID, RtmpMessage, event::event_into_raw},
    protocol::{MessageType, RawMessage, UserControlMessageKind},
};

impl RtmpMessage {
    pub fn into_raw(self) -> Result<RawMessage, SerializationError> {
        let result = match self {
            RtmpMessage::WindowAckSize { window_size } => RawMessage {
                msg_type: MessageType::WindowAckSize,
                stream_id: 0,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&window_size.to_be_bytes()[..]),
            },
            RtmpMessage::SetPeerBandwidth {
                bandwidth,
                limit_type,
            } => RawMessage {
                msg_type: MessageType::SetPeerBandwidth,
                stream_id: 0,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::from([&bandwidth.to_be_bytes()[..], &[limit_type]].concat()),
            },
            RtmpMessage::StreamBegin { stream_id } => RawMessage {
                msg_type: MessageType::UserControl,
                stream_id: 0,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::from(
                    [
                        &UserControlMessageKind::StreamBegin.into_raw().to_be_bytes()[..],
                        &stream_id.to_be_bytes(),
                    ]
                    .concat(),
                ),
            },
            RtmpMessage::CommandMessageAmf3 { values, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf3,
                stream_id,
                chunk_stream_id: MAIN_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: encode_avmplus_values(&values)?,
            },
            RtmpMessage::CommandMessageAmf0 { values, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf0,
                stream_id,
                chunk_stream_id: MAIN_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: encode_amf0_values(&values)?,
            },
            RtmpMessage::SetChunkSize { chunk_size } => RawMessage {
                msg_type: MessageType::SetChunkSize,
                stream_id: 0,
                chunk_stream_id: RESERVED_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&chunk_size.to_be_bytes()[..]),
            },
            RtmpMessage::Event { event, stream_id } => event_into_raw(event, stream_id)?,
        };
        Ok(result)
    }
}
