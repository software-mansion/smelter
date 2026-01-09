use bytes::Bytes;

pub(crate) mod message_reader;
pub(crate) mod message_writer;

pub struct RtmpMessage {
    pub type_id: u8,
    pub stream_id: u32,
    pub timestamp: u32,
    pub payload: Bytes,
}
