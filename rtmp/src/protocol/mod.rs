use bytes::Bytes;

use crate::ParseError;

mod buffered_stream_reader;
mod chunk;
pub(crate) mod handshake;
pub(crate) mod message_reader;
pub(crate) mod message_writer;

#[derive(Debug)]
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
    SetChunkSize,     // 1
    AbortMessage,     // 2
    Acknowledgement,  // 3
    UserControl,      // 4
    WindowAckSize,    // 5
    SetPeerBandwidth, // 6

    Audio, // 8
    Video, // 9

    DataMessageAmf3,    // 15 (0x0F)
    SharedObjectAmf3,   // 16 (0x10)
    CommandMessageAmf3, // 17 (0x11)
    DataMessageAmf0,    // 18 (0x12)
    SharedObjectAmf0,   // 19 (0x13)
    CommandMessageAmf0, // 20 (0x14)

    AggregateMessage, // 22 (0x16)
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
            15 => Ok(MessageType::DataMessageAmf3),
            16 => Ok(MessageType::SharedObjectAmf3),
            17 => Ok(MessageType::CommandMessageAmf3),
            18 => Ok(MessageType::DataMessageAmf0),
            19 => Ok(MessageType::SharedObjectAmf0),
            20 => Ok(MessageType::CommandMessageAmf0),
            22 => Ok(MessageType::AggregateMessage),
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
            MessageType::DataMessageAmf3 => 15,
            MessageType::SharedObjectAmf3 => 16,
            MessageType::CommandMessageAmf3 => 17,
            MessageType::DataMessageAmf0 => 18,
            MessageType::SharedObjectAmf0 => 19,
            MessageType::CommandMessageAmf0 => 20,
            MessageType::AggregateMessage => 22,
        }
    }
}
