use std::{
    cmp::min,
    collections::{HashMap, VecDeque},
    net::TcpStream,
    sync::{Arc, atomic::AtomicBool},
};

use bytes::Bytes;

use crate::{
    RtmpError,
    protocol::{buffered_stream_reader::BufferedReader, message_reader::PayloadAccumulator},
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
    pub fmt: ChunkType,
    pub cs_id: u32,
    pub timestamp: u32,
    pub timestamp_delta: u32,
    pub msg_len: u32,
    pub msg_type_id: u8,
    pub msg_stream_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ChunkType {
    Full = 0,
    NoMessageStreamId = 1,
    TimestampOnly = 2,
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

pub(crate) struct RtmpChunkReader {
    reader: BufferedReader,
    prev_headers: HashMap<u32, ChunkHeader>,
    chunk_size: usize,
}

impl RtmpChunkReader {
    pub fn new(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        Self {
            reader: BufferedReader::new(socket, should_close),
            prev_headers: HashMap::new(),
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

    fn try_parse_chunk(
        &mut self,
        accumulators: &HashMap<u32, PayloadAccumulator>,
    ) -> Result<(RtmpChunk, usize), ParseChunkError> {
        let data = self.reader.data();
        let mut cursor = 0;

        if data.is_empty() {
            return Err(ParseChunkError::NotEnoughData);
        }

        // basic header
        let first_byte = data[cursor];
        cursor += 1;

        let fmt = ChunkType::from(first_byte >> 6);
        let cs_id_marker = first_byte & 0x3F;

        let cs_id = match cs_id_marker {
            0 => {
                if data.len() < cursor + 1 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let b = data[cursor];
                cursor += 1;
                64 + (b as u32)
            }
            1 => {
                if data.len() < cursor + 2 {
                    return Err(ParseChunkError::NotEnoughData);
                }
                let b0 = data[cursor];
                let b1 = data[cursor + 1];
                cursor += 2;
                64 + (b0 as u32) + ((b1 as u32) * 256)
            }
            n => n as u32,
        };

        // prev header
        let (mut timestamp, mut timestamp_delta, mut msg_len, mut msg_type_id, mut msg_stream_id) =
            match self.prev_headers.get(&cs_id) {
                Some(p) => (
                    p.timestamp,
                    p.timestamp_delta,
                    p.msg_len,
                    p.msg_type_id,
                    p.msg_stream_id,
                ),
                None => (0, 0, 0, 0, 0),
            };

        // message header
        let mut has_extended_ts = false;

        match fmt {
            ChunkType::Full => {
                // Type 0 (11 bytes)
                if data.len() < cursor + 11 {
                    return Err(ParseChunkError::NotEnoughData);
                }

                // [timestamp(3)] [msg_len(3)] [msg_type_id(1)] [msg_stream_id(4 LE)]
                let ts_raw = read_u24(data, cursor);
                msg_len = read_u24(data, cursor + 3);
                msg_type_id = data[cursor + 6];
                msg_stream_id = read_u32_le(data, cursor + 7);
                cursor += 11;

                if ts_raw == 0xFFFFFF {
                    has_extended_ts = true;
                } else {
                    timestamp = ts_raw;
                }
                timestamp_delta = 0;
            }
            ChunkType::NoMessageStreamId => {
                // Type 1 (7 bytes)
                if data.len() < cursor + 7 {
                    return Err(ParseChunkError::NotEnoughData);
                }

                // [timestamp_delta(3)] [msg_len(3)] [msg_type_id(1)]
                let ts_delta_raw = read_u24(data, cursor);
                msg_len = read_u24(data, cursor + 3);
                msg_type_id = data[cursor + 6];
                cursor += 7;

                if ts_delta_raw == 0xFFFFFF {
                    has_extended_ts = true;
                } else {
                    timestamp_delta = ts_delta_raw;
                }
            }
            ChunkType::TimestampOnly => {
                // Type 2 (3 bytes)
                if data.len() < cursor + 3 {
                    return Err(ParseChunkError::NotEnoughData);
                }

                // [timestamp_delta(3)]
                let ts_delta_raw = read_u24(data, cursor);
                cursor += 3;

                if ts_delta_raw == 0xFFFFFF {
                    has_extended_ts = true;
                } else {
                    timestamp_delta = ts_delta_raw;
                }
            }
            ChunkType::NoHeader => {
                // Type 3 (0 bytes)
                if timestamp_delta == 0xFFFFFF || timestamp == 0xFFFFFF {
                    has_extended_ts = true;
                }
            }
        }

        // extended timestamp
        if has_extended_ts {
            if data.len() < cursor + 4 {
                return Err(ParseChunkError::NotEnoughData);
            }
            let extended_ts = read_u32_be(data, cursor);
            cursor += 4;

            match fmt {
                ChunkType::Full => timestamp = extended_ts,
                _ => timestamp_delta = extended_ts,
            }
        }

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

        self.prev_headers.insert(cs_id, header.clone());

        let mut payload_vec = Vec::with_capacity(chunk_payload_size);
        for i in 0..chunk_payload_size {
            payload_vec.push(data[cursor + i]);
        }
        let payload = Bytes::from(payload_vec);
        cursor += chunk_payload_size;

        Ok((RtmpChunk { header, payload }, cursor))
    }
}

fn read_u24(data: &VecDeque<u8>, start: usize) -> u32 {
    let b0 = data[start] as u32;
    let b1 = data[start + 1] as u32;
    let b2 = data[start + 2] as u32;
    (b0 << 16) | (b1 << 8) | b2
}

fn read_u32_be(data: &VecDeque<u8>, start: usize) -> u32 {
    let b0 = data[start] as u32;
    let b1 = data[start + 1] as u32;
    let b2 = data[start + 2] as u32;
    let b3 = data[start + 3] as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

fn read_u32_le(data: &VecDeque<u8>, start: usize) -> u32 {
    let b0 = data[start] as u32;
    let b1 = data[start + 1] as u32;
    let b2 = data[start + 2] as u32;
    let b3 = data[start + 3] as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}
