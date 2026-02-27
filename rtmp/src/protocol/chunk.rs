use std::collections::VecDeque;

use bytes::{BufMut, Bytes, BytesMut};

use crate::RtmpMessageSerializeError;

#[derive(Debug)]
pub(crate) enum ParseChunkError {
    NotEnoughData,
    MalformedStream(String),
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
        let fmt = ChunkType::from_raw((data[0] & 0b1100_0000) >> 6);

        let cs_id_marker = data[0] & 0b0011_1111;
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

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        let fmt_bits = ((self.fmt as u8) & 0b0000_0011) << 6;
        match self.cs_id {
            0 | 1 => Err(RtmpMessageSerializeError::InternalError(
                "Chunk stream ID 0 and 1 are reserved.".into(),
            )),
            2..=63 => {
                let mut buf = BytesMut::with_capacity(1);
                buf.extend_from_slice(&[fmt_bits | self.cs_id as u8]);
                Ok(buf.freeze())
            }
            64..=319 => {
                let mut buf = BytesMut::with_capacity(2);
                buf.extend_from_slice(&[fmt_bits, (self.cs_id - 64) as u8]);
                Ok(buf.freeze())
            }
            _ => {
                let id = (self.cs_id - 64) as u16;
                let le = id.to_le_bytes();
                let mut buf = BytesMut::with_capacity(3);
                buf.extend_from_slice(&[fmt_bits | 0b0000_0001, le[0], le[1]]);
                Ok(buf.freeze())
            }
        }
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
    // Type 1 - 7 bytes
    NoMessageStreamId {
        // 3 bytes
        timestamp_delta: u32,
        // 3 bytes
        msg_len: u32,
        // 1 byte
        msg_type_id: u8,
    },
    // Type 2 - 3 bytes
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

    pub fn chunk_type(&self) -> ChunkType {
        match self {
            ChunkMessageHeader::Full { .. } => ChunkType::Full,
            ChunkMessageHeader::NoMessageStreamId { .. } => ChunkType::NoMessageStreamId,
            ChunkMessageHeader::TimestampOnly { .. } => ChunkType::TimestampOnly,
            ChunkMessageHeader::NoHeader => ChunkType::NoHeader,
        }
    }

    pub fn serialize(self) -> Bytes {
        match self {
            ChunkMessageHeader::Full {
                timestamp,
                msg_len,
                msg_type_id,
                msg_stream_id,
            } => {
                let mut buf = BytesMut::with_capacity(11);
                buf.put(&timestamp.to_be_bytes()[1..4]);
                buf.put(&msg_len.to_be_bytes()[1..4]);
                buf.put_u8(msg_type_id);
                buf.put(&msg_stream_id.to_le_bytes()[..]);
                buf.freeze()
            }
            ChunkMessageHeader::NoMessageStreamId {
                timestamp_delta,
                msg_len,
                msg_type_id,
            } => {
                let mut buf = BytesMut::with_capacity(7);
                buf.put(&timestamp_delta.to_be_bytes()[1..4]);
                buf.put(&msg_len.to_be_bytes()[1..4]);
                buf.put_u8(msg_type_id);
                buf.freeze()
            }
            ChunkMessageHeader::TimestampOnly { timestamp_delta } => {
                Bytes::copy_from_slice(&timestamp_delta.to_be_bytes()[1..4])
            }
            ChunkMessageHeader::NoHeader => Bytes::new(),
        }
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
            ChunkHeaderTimestamp::Timestamp(ts) => *ts == 0x00FFFFFF,
            ChunkHeaderTimestamp::Delta(ts) => *ts == 0x00FFFFFF,
        }
    }

    pub fn value(&self) -> u32 {
        match self {
            ChunkHeaderTimestamp::Timestamp(ts) => *ts,
            ChunkHeaderTimestamp::Delta(ts) => *ts,
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
    pub fn from_msg(prev: Option<Self>, msg_header: ChunkMessageHeader) -> Result<Self, String> {
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
            (_, None) => Err("Type-0 header needs to be a first packet in a chunk stream".into()),
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
