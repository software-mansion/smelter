use std::{
    cmp::min,
    collections::{HashMap, VecDeque},
    net::TcpStream,
    sync::{Arc, atomic::AtomicBool},
    usize,
};

use bytes::Bytes;

use crate::{
    RtmpError,
    protocol::{
        MessageType, buffered_stream_reader::BufferedReader, message_reader::PayloadAccumulator,
    },
};

const MAX_MESSAGE_SIZE: usize = 5 * 1024 * 1024; // 5 MB
const DEFAULT_CHUNK_SIZE: usize = 128;

#[derive(Debug, Clone)]
pub(crate) struct RtmpChunk {
    pub header: ChunkHeader,
    pub payload: Bytes,
}

enum ParseChunkError {
    NotEnoughData,
    RtmpError(RtmpError),
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkHeader {
    /// Basic Header

    /// Format 2 bits
    pub fmt: ChunkType,
    /// Chunk stream ID - 6, 14 or 22 bits (depends on first 6 bits)
    pub cs_id: u32,

    /// Message Header
    pub timestamp: u32,
    pub timestamp_delta: u32,
    pub msg_len: u32,
    pub msg_type_id: u8,
    pub msg_stream_id: u32,
}

#[derive(Debug, Clone)]
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
    timestamp: Option<u32>,
    msg_len: Option<u32>,
    msg_type_id: Option<u8>,
    msg_stream_id: Option<u32>,
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

    pub fn read_chunk(
        &mut self,
        accumulators: &HashMap<u32, PayloadAccumulator>,
    ) -> Result<RtmpChunk, RtmpError> {
        loop {
            match self.try_parse_chunk(accumulators) {
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

    // let marker = [6 bits]
    // If marker == 0 Then cs_id = [next 8 bits] + 64
    // If marker == 1 Then cs_id = [next 16 bits] + 64
    // Else cs_id = x
    fn try_read_cs_id(&mut self, data: &VecDeque<u8>) -> Result<(u32, usize), ParseChunkError> {
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
        &mut self,
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

    fn try_parse_chunk(
        &mut self,
        accumulators: &HashMap<u32, PayloadAccumulator>,
    ) -> Result<(RtmpChunk, usize), ParseChunkError> {
        let data = self.reader.data();

        if data.is_empty() {
            return Err(ParseChunkError::NotEnoughData);
        }

        // basic header 1, 2 or 3 bytes
        let fmt = ChunkType::from((data[1] & 0b1100_0000) >> 6);
        let (cs_id, offset) = self.try_read_cs_id(&data)?;

        let (msg_header, offset) = self.try_read_msg_header(fmt, &data, offset)?;

        let mut context = self.context.entry(cs_id).or_insert_with(Default::default);
        let (timestamp, offset) = match msg_header.has_extended_timestamp(&context) {
            true => {
                if data.len() < offset + 4 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                (read_u32_be(data, offset), offset + 4)
            }
            false => {
                // TODO: not sure what to do if timestamp was not provided in
                // previous chunks
                (msg_header.timestamp().unwrap_or(0), offset)
            }
        };

        context.update(msg_header);

        // message length from previous chunks
        let current_acc = accumulators
            .get(&cs_id)
            .map(|a| a.current_len())
            .unwrap_or(0);
        let is_continuation = current_acc > 0;

        // calculate timestamp (only advance on new message)
        if fmt != ChunkType::Full && !is_continuation {
            timestamp = timestamp.wrapping_add(timestamp_delta);
        }

        if msg_len as usize > MAX_MESSAGE_SIZE {
            return Err(ParseChunkError::RtmpError(RtmpError::MessageTooLarge(
                msg_len,
            )));
        }

        let remaining_for_message = (msg_len as usize).saturating_sub(current_acc);
        let chunk_payload_size = min(remaining_for_message, self.chunk_size);

        if data.len() < cursor + chunk_payload_size {
            return Err(ParseChunkError::NotEnoughData);
        }

        let header = ChunkHeader {
            fmt,
            cs_id,
            timestamp,
            timestamp_delta,
            msg_len,
            msg_type_id,
            msg_stream_id,
        };

        self.context.insert(cs_id, header.clone());

        let mut payload_vec = Vec::with_capacity(chunk_payload_size);
        for i in 0..chunk_payload_size {
            payload_vec.push(data[cursor + i]);
        }
        let payload = Bytes::from(payload_vec);
        cursor += chunk_payload_size;

        Ok((RtmpChunk { header, payload }, cursor))
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
}

impl ChunkStreamContext {
    fn update(&mut self, header: MessageHeader) {
        match header {
            MessageHeader::Full {
                timestamp,
                msg_len,
                msg_type_id,
                msg_stream_id,
            } => {
                self.timestamp = Some(timestamp);
                self.msg_len = Some(msg_len);
                self.msg_type_id = Some(msg_type_id);
                self.msg_stream_id = Some(msg_stream_id);
            }
            MessageHeader::NoMessageStreamId {
                timestamp_delta,
                msg_len,
                msg_type_id,
            } => {
                self.timestamp = Some(timestamp_delta);
                self.msg_len = Some(msg_len);
                self.msg_type_id = Some(msg_type_id);
            }
            MessageHeader::TimestampOnly { timestamp_delta } => {
                self.timestamp = Some(timestamp_delta);
            }
            MessageHeader::NoHeader => (),
        };
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
