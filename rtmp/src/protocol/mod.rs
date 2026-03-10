use bytes::Bytes;

use crate::RtmpMessageParseError;

pub(crate) mod byte_stream;
mod chunk;
pub(crate) mod handshake;
pub(crate) mod message_stream;

#[derive(Debug)]
pub(crate) struct RawMessage {
    pub msg_type: u8,
    pub stream_id: u32,
    pub chunk_stream_id: u32,
    pub timestamp: u32,
    pub payload: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    // https://rtmp.veriskope.com/docs/spec/#54-protocol-control-messages
    // Protocol Control Messages (1-6)
    SetChunkSize,
    AbortMessage,
    Acknowledgement,
    UserControl,
    WindowAckSize,
    SetPeerBandwidth,

    Audio,
    Video,
    DataMessageAmf3,
    DataMessageAmf0,

    CommandMessageAmf3,
    CommandMessageAmf0,

    AggregateMessage,
}

impl MessageType {
    pub(crate) fn try_from_raw(value: u8) -> Result<Self, RtmpMessageParseError> {
        match value {
            1 => Ok(MessageType::SetChunkSize),
            2 => Ok(MessageType::AbortMessage),
            3 => Ok(MessageType::Acknowledgement),
            4 => Ok(MessageType::UserControl),
            5 => Ok(MessageType::WindowAckSize),
            6 => Ok(MessageType::SetPeerBandwidth),
            8 => Ok(MessageType::Audio),
            9 => Ok(MessageType::Video),
            15 => Ok(MessageType::DataMessageAmf3),
            17 => Ok(MessageType::CommandMessageAmf3),
            18 => Ok(MessageType::DataMessageAmf0),
            20 => Ok(MessageType::CommandMessageAmf0),
            22 => Ok(MessageType::AggregateMessage),
            _ => Err(RtmpMessageParseError::InvalidMessageType(value)),
        }
    }

    pub(crate) fn into_raw(self) -> u8 {
        match self {
            MessageType::SetChunkSize => 1,
            MessageType::AbortMessage => 2,
            MessageType::Acknowledgement => 3,
            MessageType::UserControl => 4,
            MessageType::WindowAckSize => 5,
            MessageType::SetPeerBandwidth => 6,
            MessageType::Audio => 8,
            MessageType::Video => 9,
            MessageType::DataMessageAmf3 => 15,
            MessageType::CommandMessageAmf3 => 17,
            MessageType::DataMessageAmf0 => 18,
            MessageType::CommandMessageAmf0 => 20,
            MessageType::AggregateMessage => 22,
        }
    }
}
