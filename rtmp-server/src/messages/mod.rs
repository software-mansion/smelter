use bytes::{BufMut, BytesMut};

pub mod parser;

pub use parser::{MessageParser, MessageParserError};
use thiserror::Error;

use crate::amf0::{Encoder as Amf0Encoder, parser::AmfValue};

// TODO split this mod into files

#[derive(Debug, Clone)]
pub enum RtmpMessage {
    SetChunkSize(u32),
    Acknowledgement(u32),
    WindowAcknowledgementSize(u32),
    SetPeerBandwidth {
        size: u32,
        limit_type: u8,
    },
    UserControl {
        event_type: u16,
        data: Vec<u8>,
    },
    Command {
        name: String,
        transaction_id: f64,
        data: Vec<AmfValue>,
    },
    Audio(Vec<u8>),
    Video(Vec<u8>),
    DataMessage(Vec<AmfValue>),
    Unknown {
        type_id: u8,
        data: Vec<u8>,
    },
}

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Invalid chunk size: {0} (must be between 1 and 0x7FFFFFFF)")]
    InvalidChunkSize(u32),

    #[error("Invalid window acknowledgement size: {0}")]
    InvalidWindowAckSize(u32),

    #[error("Invalid peer bandwidth size: {0}")]
    InvalidPeerBandwidth(u32),

    #[error("Invalid limit type: {0} (must be 0, 1, or 2)")]
    InvalidLimitType(u8),

    #[error("AMF encoding error: {0}")]
    AmfEncodingError(String),

    #[error("Data too large: {size} bytes (max: {max})")]
    DataTooLarge { size: usize, max: usize },
}

// message type IDs
pub const MSG_SET_CHUNK_SIZE: u8 = 1;
pub const MSG_ABORT: u8 = 2;
pub const MSG_ACKNOWLEDGEMENT: u8 = 3;
pub const MSG_USER_CONTROL: u8 = 4;
pub const MSG_WINDOW_ACK_SIZE: u8 = 5;
pub const MSG_SET_PEER_BANDWIDTH: u8 = 6;
pub const MSG_AUDIO: u8 = 8;
pub const MSG_VIDEO: u8 = 9;
pub const MSG_DATA_AMF0: u8 = 18;
pub const MSG_COMMAND_AMF0: u8 = 20;

impl RtmpMessage {
    pub fn type_id(&self) -> u8 {
        match self {
            RtmpMessage::SetChunkSize(_) => MSG_SET_CHUNK_SIZE,
            RtmpMessage::Acknowledgement(_) => MSG_ACKNOWLEDGEMENT,
            RtmpMessage::WindowAcknowledgementSize(_) => MSG_WINDOW_ACK_SIZE,
            RtmpMessage::SetPeerBandwidth { .. } => MSG_SET_PEER_BANDWIDTH,
            RtmpMessage::UserControl { .. } => MSG_USER_CONTROL,
            RtmpMessage::Command { .. } => MSG_COMMAND_AMF0,
            RtmpMessage::DataMessage(_) => MSG_DATA_AMF0,
            RtmpMessage::Audio(_) => MSG_AUDIO,
            RtmpMessage::Video(_) => MSG_VIDEO,
            RtmpMessage::Unknown { type_id, .. } => *type_id,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();
        match self {
            RtmpMessage::SetChunkSize(size) => {
                buf.put_u32(*size);
            }
            RtmpMessage::Acknowledgement(seq) => {
                buf.put_u32(*seq);
            }
            RtmpMessage::WindowAcknowledgementSize(size) => {
                buf.put_u32(*size);
            }
            RtmpMessage::SetPeerBandwidth { size, limit_type } => {
                buf.put_u32(*size);
                buf.put_u8(*limit_type);
            }
            RtmpMessage::UserControl { event_type, data } => {
                buf.put_u16(*event_type);
                buf.extend_from_slice(data);
            }
            RtmpMessage::Command {
                name,
                transaction_id,
                data,
            } => {
                let mut values = vec![
                    AmfValue::String(name.clone()),
                    AmfValue::Number(*transaction_id),
                ];
                let encoder = Amf0Encoder;
                values.extend(data.clone());
                return encoder.encode(&values).expect("Error while encoding AMF0");
            }
            RtmpMessage::DataMessage(values) => {
                let encoder = Amf0Encoder;
                return encoder.encode(values).expect("Error while encoding AMF0"); // TODO better error handling
            }
            RtmpMessage::Audio(data) | RtmpMessage::Video(data) => {
                buf.extend_from_slice(data);
            }
            RtmpMessage::Unknown { data, .. } => {
                buf.extend_from_slice(data);
            }
        }
        buf.to_vec()
    }

    pub fn encode_set_chunk_size(size: u32) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(4);
        buf.put_u32(size);
        buf.to_vec()
    }

    pub fn encode_window_ack_size(size: u32) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(4);
        buf.put_u32(size);
        buf.to_vec()
    }

    pub fn encode_set_peer_bandwidth(size: u32, limit_type: u8) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(5);
        buf.put_u32(size);
        buf.put_u8(limit_type);
        buf.to_vec()
    }

    pub fn encode_user_control(event_type: u16, data: &[u8]) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(2 + data.len());
        buf.put_u16(event_type);
        buf.put_slice(data);
        buf.to_vec()
    }
}
