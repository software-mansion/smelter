use bytes::Bytes;

use crate::{
    RtmpMessageSerializeError,
    amf0::encode_amf_values,
    message::{
        CONTROL_MESSAGE_STREAM_ID, MAIN_CHUNK_STREAM_ID, PROTOCOL_CHUNK_STREAM_ID,
        RtmpMessageOutgoing,
    },
    protocol::{MessageType, RawMessage},
};

impl RtmpMessageOutgoing {
    pub fn into_raw(self) -> Result<RawMessage, RtmpMessageSerializeError> {
        let result = match self {
            RtmpMessageOutgoing::WindowAckSize { window_size } => RawMessage {
                msg_type: MessageType::WindowAckSize.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: PROTOCOL_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&window_size.to_be_bytes()[..]),
            },
            RtmpMessageOutgoing::Acknowledgement { bytes_received } => RawMessage {
                msg_type: MessageType::Acknowledgement.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: PROTOCOL_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&bytes_received.to_be_bytes()[..]),
            },
            RtmpMessageOutgoing::SetPeerBandwidth {
                bandwidth,
                limit_type,
            } => RawMessage {
                msg_type: MessageType::SetPeerBandwidth.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: PROTOCOL_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::from([&bandwidth.to_be_bytes()[..], &[limit_type]].concat()),
            },
            RtmpMessageOutgoing::UserControl(msg) => RawMessage {
                msg_type: MessageType::UserControl.into_raw(),
                stream_id: CONTROL_MESSAGE_STREAM_ID,
                chunk_stream_id: PROTOCOL_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: msg.into_raw(),
            },
            RtmpMessageOutgoing::SetChunkSize { chunk_size } => RawMessage {
                msg_type: MessageType::SetChunkSize.into_raw(),
                stream_id: 0,
                chunk_stream_id: PROTOCOL_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&chunk_size.to_be_bytes()[..]),
            },
            RtmpMessageOutgoing::CommandMessage { msg, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf0.into_raw(),
                stream_id,
                chunk_stream_id: MAIN_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: msg.into_amf0_bytes()?,
            },
            RtmpMessageOutgoing::DataMessage { data, stream_id } => RawMessage {
                msg_type: MessageType::DataMessageAmf0.into_raw(),
                stream_id,
                chunk_stream_id: MAIN_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: encode_amf_values(&data.into_amf_values())?,
            },
            RtmpMessageOutgoing::Video {
                video: msg,
                stream_id,
                serialization_mode,
            } => msg.into_raw(stream_id, serialization_mode)?,
            RtmpMessageOutgoing::Audio {
                audio: msg,
                stream_id,
                channels,
                serialization_mode,
            } => msg.into_raw(stream_id, channels, serialization_mode)?,
        };
        Ok(result)
    }
}
