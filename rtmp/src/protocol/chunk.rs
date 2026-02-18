use std::{
    collections::{HashMap, VecDeque},
    net::TcpStream,
    sync::{Arc, atomic::AtomicBool},
    usize,
};

use bytes::Bytes;

use crate::{
    RtmpError,
    protocol::{
        MessageType, RawMessage, buffered_stream_reader::BufferedReader,
        message_reader::PayloadAccumulator,
    },
};

const MAX_MESSAGE_SIZE: usize = 5 * 1024 * 1024; // 5 MB
const DEFAULT_CHUNK_SIZE: usize = 128;

#[derive(Debug, Clone)]
pub(crate) struct RtmpChunk {
    pub header: Chunk,
    pub payload: Bytes,
}

#[derive(thiserror::Error, Debug)]
enum ParseChunkError {
    #[error("Not enough data")]
    NotEnoughData,

    #[error(transparent)]
    RtmpError(#[from] RtmpError),
}

#[derive(Debug, Clone)]
pub(crate) struct Chunk {
    /// Basic Header

    /// Format 2 bits
    pub fmt: ChunkType,
    /// Chunk stream ID - 6, 14 or 22 bits (depends on first 6 bits)
    pub cs_id: u32,

    /// Message Header
    pub msg_header: MessageHeader,

    /// Real timestamp (includes extra timestamp)
    pub timestamp: u32,

    pub payload: Bytes,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MessageHeader {
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

impl From<u8> for ChunkType {
    fn from(v: u8) -> Self {
        match v {
            0 => ChunkType::Full,
            1 => ChunkType::NoMessageStreamId,
            2 => ChunkType::TimestampOnly,
            3 => ChunkType::NoHeader,
            _ => unreachable!("fmt field is only 2 bits"),
        }
    }
}

#[derive(Debug, Default)]
struct ChunkStreamContext {
    timestamp: u32,
    msg_len: u32,
    msg_type_id: u8,
    msg_stream_id: u32,

    payload_acc: PayloadAccumulator,
}

pub(crate) struct RtmpChunkReader {
    reader: BufferedReader,
    context: HashMap<u32, ChunkStreamContext>,
    chunk_size: usize,
}

impl RtmpChunkReader {
    pub fn new(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        Self {
            reader: BufferedReader::new(socket, should_close),
            context: HashMap::new(),
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    pub fn read_chunk(&mut self) -> Result<RawMessage, RtmpError> {
        loop {
            match self.try_parse_chunk() {
                Ok((chunk, len_to_read)) => {
                    self.reader.read_bytes(len_to_read)?;
                    return Ok(chunk);
                }
                Err(ParseChunkError::NotEnoughData) => {
                    let current_len = self.reader.data().len();
                    self.reader.read_until_buffer_size(current_len + 1)?;
                }
                Err(ParseChunkError::RtmpError(e)) => return Err(e),
            }
        }
    }

    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }

    fn try_parse_chunk(&mut self) -> Result<RawMessage, ParseChunkError> {
        let data = self.reader.data();

        if data.is_empty() {
            return Err(ParseChunkError::NotEnoughData);
        }

        // basic header 1, 2 or 3 bytes
        let fmt = ChunkType::from((data[1] & 0b1100_0000) >> 6);
        let (cs_id, offset) = self.try_read_cs_id(&data)?;

        let (msg_header, offset) = self.try_read_msg_header(fmt, &data, offset)?;

        match msg_header {
            MessageHeader::Full {
                timestamp,
                msg_len,
                msg_type_id,
                msg_stream_id,
            } => self.context.entry(cs_id).or_insert_with(Default::default),
            _ => {}
        }
        let context = self.context.entry(cs_id).or_insert_with(Default::default);
        let (extended_timestamp, offset) = match msg_header.has_extended_timestamp(&context) {
            true => {
                if data.len() < offset + 4 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                (Some(read_u32_be(data, offset)), offset + 4)
            }
            false => (None, offset),
        };

        let msg_len = msg_header.msg_len(context)?;

        if msg_len as usize > MAX_MESSAGE_SIZE {
            return Err(RtmpError::MessageTooLarge(msg_len).into());
        }

        let payload_size = (msg_len as usize)
            .saturating_sub(context.payload_acc.len())
            .min(self.chunk_size);

        if data.len() < offset + payload_size {
            return Err(ParseChunkError::NotEnoughData);
        }

        context.update(msg_header, extended_timestamp)?;

        let payload = Bytes::from_iter(data.iter().skip(offset).take(payload_size).copied());

        let chunk = Chunk {
            fmt,
            cs_id,
            msg_header,
            timestamp: context.timestamp.unwrap(),
            payload,
        };

        self.reader.read_bytes(offset + payload_size)?;

        if let Some(msg_payload) = context.payload_acc.try_pop()? {
            Ok(RawMessage {
                timestamp: context.timestamp,
                msg_type: MessageType::try_from_raw(context.msg_type_id)?,
                stream_id: context.msg_stream_id,
                payload: acc.buffer.freeze(),
            })
        } else {
            Ok(Err(ParseChunkError::NotEnoughData))
        }
    }

    // let marker = [6 bits]
    // If marker == 0 Then cs_id = [next 8 bits] + 64
    // If marker == 1 Then cs_id = [next 16 bits] + 64
    // Else cs_id = x
    fn try_read_cs_id(&self, data: &VecDeque<u8>) -> Result<(u32, usize), ParseChunkError> {
        if data.len() < 1 {
            return Err(ParseChunkError::NotEnoughData);
        }
        let cs_id_marker = data[0] & 0b00111111;
        let (cs_id, offset) = match cs_id_marker {
            0 => {
                if data.len() < 2 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let cs_id = (data[1] as u32) + 64;
                (cs_id, 2 as usize)
            }
            1 => {
                if data.len() < 3 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let cs_id = u16::from_le_bytes([data[1], data[2]]) as u32 + 64;
                (cs_id, 3 as usize)
            }
            n => (n as u32, 1 as usize),
        };
        Ok((cs_id, offset))
    }

    fn try_read_msg_header(
        &self,
        fmt: ChunkType,
        data: &VecDeque<u8>,
        offset: usize,
    ) -> Result<(MessageHeader, usize), ParseChunkError> {
        let (header, offset) = match fmt {
            ChunkType::Full => {
                // Type 0 (11 bytes)
                // [timestamp(3)] [msg_len(3)] [msg_type_id(1)] [msg_stream_id(4 LE)]
                if data.len() < offset + 11 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let header = MessageHeader::Full {
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
                let header = MessageHeader::NoMessageStreamId {
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
                let header = MessageHeader::TimestampOnly {
                    timestamp_delta: read_u24(data, offset),
                };
                (header, offset + 3)
            }
            ChunkType::NoHeader => (MessageHeader::NoHeader, offset),
        };
        Ok((header, offset))
    }
}

impl MessageHeader {
    fn has_extended_timestamp(&self, context: &ChunkStreamContext) -> bool {
        match self {
            MessageHeader::Full { timestamp, .. } => *timestamp == 0xFFFFFF,
            MessageHeader::NoMessageStreamId {
                timestamp_delta, ..
            } => *timestamp_delta == 0xFFFFFF,
            MessageHeader::TimestampOnly { timestamp_delta } => *timestamp_delta == 0xFFFFFF,
            MessageHeader::NoHeader => match context.timestamp {
                Some(prev) => prev == 0xFFFFFF,
                None => false,
            },
        }
    }

    fn timestamp(&self) -> Option<u32> {
        match self {
            MessageHeader::Full { timestamp, .. } => Some(*timestamp),
            MessageHeader::NoMessageStreamId {
                timestamp_delta, ..
            } => Some(*timestamp_delta),
            MessageHeader::TimestampOnly { timestamp_delta } => Some(*timestamp_delta),
            MessageHeader::NoHeader => None,
        }
    }

    fn msg_len(&self, context: &ChunkStreamContext) -> Result<u32, ParseChunkError> {
        match self {
            MessageHeader::Full { msg_len, .. } => Ok(*msg_len),
            MessageHeader::NoMessageStreamId { msg_len, .. } => Ok(*msg_len),
            MessageHeader::TimestampOnly { .. } | MessageHeader::NoHeader => {
                match context.msg_len {
                    Some(_) => todo!(),
                    None => Err(RtmpError::InternalError("missing msg_len")),
                }
            }
        }
    }
}

impl ChunkStreamContext {
    fn update(
        &mut self,
        header: MessageHeader,
        extended_timestamp: Option<u32>,
    ) -> Result<(), ParseChunkError> {
        match header {
            MessageHeader::Full {
                timestamp,
                msg_len,
                msg_type_id,
                msg_stream_id,
            } => {
                self.timestamp = timestamp;
                self.msg_len = msg_len;
                self.msg_type_id = msg_type_id;
                self.msg_stream_id = msg_stream_id;
            }
            MessageHeader::NoMessageStreamId {
                timestamp_delta,
                msg_len,
                msg_type_id,
            } => {
                let timestamp_delta = extended_timestamp.unwrap_or(timestamp_delta);
                self.timestamp = ts.wrapping_add(timestamp_delta);
                self.msg_len = msg_len;
                self.msg_type_id = msg_type_id;
            }
            MessageHeader::TimestampOnly { timestamp_delta } => {
                let Some(ts) = self.timestamp else {
                    return Err(RtmpError::InternalError("missing timestamp").into());
                };
                let timestamp_delta = extended_timestamp.unwrap_or(timestamp_delta);
                self.timestamp = Some(ts.wrapping_add(timestamp_delta));
            }
            MessageHeader::NoHeader => (),
        };
        Ok(())
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
