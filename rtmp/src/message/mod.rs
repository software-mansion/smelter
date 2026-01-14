use bytes::Bytes;

use crate::protocol::MessageType;

pub(crate) mod message_reader;
pub(crate) mod message_writer;

pub struct RtmpMessage {
    pub msg_type: MessageType,
    pub stream_id: u32,
    pub timestamp: u32,
    pub payload: Bytes,
}
