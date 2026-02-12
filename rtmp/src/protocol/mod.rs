use bytes::Bytes;

use crate::ParseError;

mod buffered_stream_reader;
mod chunk;
pub(crate) mod handshake;
pub(crate) mod message_reader;
pub(crate) mod message_writer;

pub(crate) struct RawMessage {
    pub msg_type: MessageType,
    pub stream_id: u32,
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
    DataMessageAmf0,

    CommandMessageAmf0,
}

impl MessageType {
    pub(crate) fn try_from_raw(value: u8) -> Result<Self, ParseError> {
        match value {
            1 => Ok(MessageType::SetChunkSize),
            2 => Ok(MessageType::AbortMessage),
            3 => Ok(MessageType::Acknowledgement),
            4 => Ok(MessageType::UserControl),
            5 => Ok(MessageType::WindowAckSize),
            6 => Ok(MessageType::SetPeerBandwidth),
            8 => Ok(MessageType::Audio),
            9 => Ok(MessageType::Video),
            18 => Ok(MessageType::DataMessageAmf0),
            20 => Ok(MessageType::CommandMessageAmf0),
            _ => Err(ParseError::UnknownMessageType(value)),
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
            MessageType::DataMessageAmf0 => 18,
            MessageType::CommandMessageAmf0 => 20,
        }
    }
}

// https://rtmp.veriskope.com/docs/spec/#717user-control-message-events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(unused)]
pub enum UserControlMessageEvent {
    StreamBegin = 0,
    #[allow(unused)]
    StreamEof = 1,
    #[allow(unused)]
    StreamDry = 2,
    #[allow(unused)]
    SetBufferLength = 3,
    #[allow(unused)]
    StreamIsRecorded = 4,
    #[allow(unused)]
    PingRequest = 6,
    #[allow(unused)]
    PingResponse = 7,
}
