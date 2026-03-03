use bytes::Bytes;

use crate::RtmpMessageParseError;

mod chunk;
pub(crate) mod handshake;
pub(crate) mod message_reader;
pub(crate) mod message_writer;
pub(crate) mod socket;
pub(crate) mod tls;

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
        }
    }
}

// https://rtmp.veriskope.com/docs/spec/#717user-control-message-events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(unused)]
pub enum UserControlMessageKind {
    StreamBegin,
    StreamEof,
    StreamDry,
    SetBufferLength,
    StreamIsRecorded,
    PingRequest,
    PingResponse,
}

impl UserControlMessageKind {
    pub fn from_raw(value: u16) -> Result<Self, RtmpMessageParseError> {
        match value {
            0 => Ok(Self::StreamBegin),
            1 => Ok(Self::StreamEof),
            2 => Ok(Self::StreamDry),
            3 => Ok(Self::SetBufferLength),
            4 => Ok(Self::StreamIsRecorded),
            6 => Ok(Self::PingRequest),
            7 => Ok(Self::PingResponse),
            _ => Err(RtmpMessageParseError::InvalidUserControlMessage(value)),
        }
    }

    pub fn into_raw(self) -> u16 {
        match self {
            Self::StreamBegin => 0,
            Self::StreamEof => 1,
            Self::StreamDry => 2,
            Self::SetBufferLength => 3,
            Self::StreamIsRecorded => 4,
            Self::PingRequest => 6,
            Self::PingResponse => 7,
        }
    }
}
