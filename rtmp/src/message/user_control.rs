use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};

use crate::RtmpMessageParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(unused)]
pub(crate) enum UserControlMessage {
    /// Server -> Client
    StreamBegin {
        stream_id: u32,
    },
    /// Server -> Client
    StreamEof {
        stream_id: u32,
    },
    /// Server -> Client
    StreamDry {
        stream_id: u32,
    },
    /// Client -> Server
    /// Send before server starts to process stream.
    SetBufferLength {
        stream_id: u32,
        buffer_duration: Duration,
    },
    /// Server -> Client
    StreamIsRecorded {
        stream_id: u32,
    },
    /// Server -> Client
    PingRequest {
        // It represents server time, but it's not clear in what
        // format. Response needs to return the same value
        timestamp: u32,
    },
    /// Client -> Server
    PingResponse {
        timestamp: u32,
    },
}

impl UserControlMessage {
    pub fn from_raw(p: &[u8]) -> Result<Self, RtmpMessageParseError> {
        if p.len() < 2 {
            return Err(RtmpMessageParseError::PayloadTooShort);
        }
        let kind = UserControlMessageKind::from_raw(u16::from_be_bytes([p[0], p[1]]))?;
        let result = match kind {
            UserControlMessageKind::StreamBegin if p.len() >= 6 => {
                let stream_id = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                Self::StreamBegin { stream_id }
            }
            UserControlMessageKind::StreamEof if p.len() >= 6 => {
                let stream_id = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                Self::StreamEof { stream_id }
            }
            UserControlMessageKind::StreamDry if p.len() >= 6 => {
                let stream_id = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                Self::StreamDry { stream_id }
            }
            UserControlMessageKind::SetBufferLength if p.len() >= 10 => {
                let stream_id = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                let duration_millis = u32::from_be_bytes([p[6], p[7], p[8], p[9]]);
                Self::SetBufferLength {
                    stream_id,
                    buffer_duration: Duration::from_millis(duration_millis as u64),
                }
            }
            UserControlMessageKind::StreamIsRecorded if p.len() >= 6 => {
                let stream_id = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                Self::StreamEof { stream_id }
            }
            UserControlMessageKind::PingRequest if p.len() >= 6 => {
                let timestamp = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                Self::PingRequest { timestamp }
            }
            UserControlMessageKind::PingResponse if p.len() >= 6 => {
                let timestamp = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                Self::PingResponse { timestamp }
            }
            _ => return Err(RtmpMessageParseError::PayloadTooShort),
        };
        Ok(result)
    }

    pub fn into_raw(self) -> Bytes {
        let mut result = BytesMut::new();
        match self {
            UserControlMessage::StreamBegin { stream_id } => {
                result.put_u16(UserControlMessageKind::StreamBegin.into_raw());
                result.put_u32(stream_id);
            }
            UserControlMessage::StreamEof { stream_id } => {
                result.put_u16(UserControlMessageKind::StreamEof.into_raw());
                result.put_u32(stream_id);
            }
            UserControlMessage::StreamDry { stream_id } => {
                result.put_u16(UserControlMessageKind::StreamDry.into_raw());
                result.put_u32(stream_id);
            }
            UserControlMessage::SetBufferLength {
                stream_id,
                buffer_duration,
            } => {
                result.put_u16(UserControlMessageKind::SetBufferLength.into_raw());
                result.put_u32(stream_id);
                result.put_u32(buffer_duration.as_millis() as u32);
            }
            UserControlMessage::StreamIsRecorded { stream_id } => {
                result.put_u16(UserControlMessageKind::StreamIsRecorded.into_raw());
                result.put_u32(stream_id);
            }
            UserControlMessage::PingRequest { timestamp } => {
                result.put_u16(UserControlMessageKind::PingRequest.into_raw());
                result.put_u32(timestamp);
            }
            UserControlMessage::PingResponse { timestamp } => {
                result.put_u16(UserControlMessageKind::PingResponse.into_raw());
                result.put_u32(timestamp);
            }
        };
        result.freeze()
    }
}

// https://rtmp.veriskope.com/docs/spec/#717user-control-message-events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(unused)]
enum UserControlMessageKind {
    StreamBegin,
    StreamEof,
    StreamDry,
    SetBufferLength,
    StreamIsRecorded,
    PingRequest,
    PingResponse,
}

impl UserControlMessageKind {
    fn from_raw(value: u16) -> Result<Self, RtmpMessageParseError> {
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

    fn into_raw(self) -> u16 {
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
