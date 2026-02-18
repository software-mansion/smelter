use std::collections::VecDeque;

use crate::{ParseError, RtmpError};

#[derive(thiserror::Error, Debug)]
pub(super) enum ParseChunkError {
    #[error("Not enough data")]
    NotEnoughData,

    #[error(transparent)]
    RtmpError(#[from] RtmpError),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ChunkType {
    // Type 0 - 11 bytes
    Full = 0,
    // Type 1 - 7 bytes
    NoMessageStreamId = 1,
    // Type 2 - 3 bytes
    TimestampOnly = 2,
    // Type 3 - 0 bytes
    NoHeader = 3,
}

impl ChunkType {
    fn from_raw(value: u8) -> Self {
        match value {
            0 => ChunkType::Full,
            1 => ChunkType::NoMessageStreamId,
            2 => ChunkType::TimestampOnly,
            3 => ChunkType::NoHeader,
            _ => unreachable!("fmt field is only 2 bits"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkBaseHeader {
    /// Format 2 bits
    pub fmt: ChunkType,

    /// Chunk stream ID - 6, 14 or 22 bits (depends on first 6 bits)
    ///
    /// If marker == 0 Then cs_id = [next 8 bits] + 64
    /// If marker == 1 Then cs_id = [next 16 bits] + 64
    /// Else cs_id = x
    pub cs_id: u32,
}

impl ChunkBaseHeader {
    pub fn try_read(data: &VecDeque<u8>) -> Result<(Self, usize), ParseChunkError> {
        if data.is_empty() {
            return Err(ParseChunkError::NotEnoughData);
        }
        let fmt = ChunkType::from_raw((data[1] & 0b1100_0000) >> 6);

        // let marker = [6 bits]

        let cs_id_marker = data[0] & 0b00111111;
        let (cs_id, offset) = match cs_id_marker {
            0 => {
                if data.len() < 2 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let cs_id = (data[1] as u32) + 64;
                (cs_id, 2_usize)
            }
            1 => {
                if data.len() < 3 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let cs_id = u16::from_le_bytes([data[1], data[2]]) as u32 + 64;
                (cs_id, 3_usize)
            }
            n => (n as u32, 1_usize),
        };
        Ok((Self { fmt, cs_id }, offset))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ChunkMessageHeader {
    // Type 0 - 11 bytes
    Full {
        // 3 bytes
        timestamp: u32,
        // 3 bytes
        msg_len: u32,
        // 1 byte
        msg_type_id: u8,
        // 4 bytes
        msg_stream_id: u32,
    },
    // Type 0 - 7 bytes
    NoMessageStreamId {
        // 3 bytes
        timestamp_delta: u32,
        // 3 bytes
        msg_len: u32,
        // 1 byte
        msg_type_id: u8,
    },
    // Type 0 - 3 bytes
    TimestampOnly {
        // 3 bytes
        timestamp_delta: u32,
    },
    NoHeader,
}

impl ChunkMessageHeader {
    pub fn try_read(
        base_header: &ChunkBaseHeader,
        data: &VecDeque<u8>,
        offset: usize,
    ) -> Result<(Self, usize), ParseChunkError> {
        let (header, offset) = match base_header.fmt {
            ChunkType::Full => {
                // Type 0 (11 bytes)
                // [timestamp(3)] [msg_len(3)] [msg_type_id(1)] [msg_stream_id(4 LE)]
                if data.len() < offset + 11 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let header = Self::Full {
                    timestamp: read_u24(data, offset),
                    msg_len: read_u24(data, offset + 3),
                    msg_type_id: data[offset + 6],
                    msg_stream_id: read_u32_le(data, offset + 7),
                };
                (header, offset + 11)
            }
            ChunkType::NoMessageStreamId => {
                // Type 1 (7 bytes)
                // [timestamp_delta(3)] [msg_len(3)] [msg_type_id(1)]
                if data.len() < offset + 7 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let header = Self::NoMessageStreamId {
                    timestamp_delta: read_u24(data, offset),
                    msg_len: read_u24(data, offset + 3),
                    msg_type_id: data[offset + 6],
                };
                (header, offset + 7)
            }
            ChunkType::TimestampOnly => {
                // Type 2 (3 bytes)
                if data.len() < offset + 3 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let header = Self::TimestampOnly {
                    timestamp_delta: read_u24(data, offset),
                };
                (header, offset + 3)
            }
            ChunkType::NoHeader => (Self::NoHeader, offset),
        };
        Ok((header, offset))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ChunkExtendedTimestamp(pub u32);

impl ChunkExtendedTimestamp {
    pub fn try_read(data: &VecDeque<u8>, offset: usize) -> Result<(Self, usize), ParseChunkError> {
        if data.len() < offset + 4 {
            return Err(ParseChunkError::NotEnoughData);
        }
        let timestamp = read_u32_be(data, offset);
        Ok((Self(timestamp), offset + 4))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChunkHeaderTimestamp {
    Timestamp(u32),
    Delta(u32),
}

impl ChunkHeaderTimestamp {
    pub fn has_extended(&self) -> bool {
        match self {
            ChunkHeaderTimestamp::Timestamp(ts) => *ts == 0xFFFFFF,
            ChunkHeaderTimestamp::Delta(ts) => *ts == 0xFFFFFF,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VirtualMessageHeader {
    pub msg_len: u32,
    pub msg_type_id: u8,
    pub msg_stream_id: u32,
    pub timestamp: ChunkHeaderTimestamp,
}

impl VirtualMessageHeader {
    pub fn from_msg(prev: Option<Self>, msg_header: ChunkMessageHeader) -> Result<Self, RtmpError> {
        match (msg_header, prev) {
            (
                ChunkMessageHeader::Full {
                    timestamp,
                    msg_len,
                    msg_type_id,
                    msg_stream_id,
                },
                None | Some(_),
            ) => Ok(Self {
                msg_len,
                msg_type_id,
                msg_stream_id,
                timestamp: ChunkHeaderTimestamp::Timestamp(timestamp),
            }),
            (
                ChunkMessageHeader::NoMessageStreamId {
                    timestamp_delta,
                    msg_len,
                    msg_type_id,
                },
                Some(mut prev),
            ) => {
                prev.timestamp = ChunkHeaderTimestamp::Delta(timestamp_delta);
                prev.msg_len = msg_len;
                prev.msg_type_id = msg_type_id;
                Ok(prev)
            }
            (ChunkMessageHeader::TimestampOnly { timestamp_delta }, Some(mut prev)) => {
                prev.timestamp = ChunkHeaderTimestamp::Delta(timestamp_delta);
                Ok(prev)
            }
            (ChunkMessageHeader::NoHeader, Some(mut prev)) => {
                // If we send Type-3 just after Type-0 then the second packet delta
                // will be the same as an absolute timestamp  (See 5.3.1.2.4)
                prev.timestamp = match prev.timestamp {
                    ChunkHeaderTimestamp::Timestamp(ts) => ChunkHeaderTimestamp::Delta(ts),
                    ChunkHeaderTimestamp::Delta(delta) => ChunkHeaderTimestamp::Delta(delta),
                };
                Ok(prev)
            }
            (_, None) => Err(ParseError::MalformedPacket(
                "Type-0 header needs to be a first packet in a chunk stream",
            )
            .into()),
        }
    }
}

fn read_u24(data: &VecDeque<u8>, offset: usize) -> u32 {
    u32::from_be_bytes([0, data[offset], data[offset + 1], data[offset + 2]])
}

fn read_u32_be(data: &VecDeque<u8>, offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_u32_le(data: &VecDeque<u8>, offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
