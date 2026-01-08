use crate::{
    buffered_stream_reader::BufferedReader, error::RtmpError, message_reader::PayloadAccumulator,
};
use bytes::Bytes;
use std::{
    cmp::min,
    collections::HashMap,
    net::TcpStream,
    sync::{Arc, atomic::AtomicBool},
};

const MAX_MESSAGE_SIZE: usize = 5 * 1024 * 1024; // 5 MB
const DEFAULT_CHUNK_SIZE: usize = 128;

#[derive(Debug, Clone)]
pub(crate) struct RtmpChunk {
    pub header: ChunkHeader,
    pub payload: Bytes,
}

#[allow(unused)]
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
        let required_size = loop {
            match self.peek_chunk_size(accumulators)? {
                Some(size) => {
                    if self.reader.data().len() >= size {
                        break size;
                    }
                    self.reader.read_until_buffer_size(size)?;
                }
                None => {
                    let current = self.reader.data().len();
                    self.reader.read_until_buffer_size(current + 1)?;
                }
            }
        };

        let chunk_data = self.reader.read_bytes(required_size)?;
        self.parse_chunk_data(chunk_data, accumulators)
    }

    fn peek_chunk_size(
        &self,
        accumulators: &HashMap<u32, PayloadAccumulator>,
    ) -> Result<Option<usize>, RtmpError> {
        let buf = self.reader.data();
        if buf.is_empty() {
            return Ok(None);
        }

        // basic header
        let first_byte = buf[0];
        let fmt_byte = first_byte >> 6;
        let cs_id_initial = first_byte & 0x3F;

        let basic_header_len = match cs_id_initial {
            0 => 2,
            1 => 3,
            _ => 1,
        };

        if buf.len() < basic_header_len {
            return Ok(None);
        }

        let cs_id = match cs_id_initial {
            0 => 64 + (buf[1] as u32),
            1 => 64 + (buf[1] as u32) + ((buf[2] as u32) * 256),
            n => n as u32,
        };

        let fmt = ChunkType::from(fmt_byte);

        // message header
        let msg_header_len = match fmt {
            ChunkType::Full => 11,
            ChunkType::NoMessageStreamId => 7,
            ChunkType::TimestampOnly => 3,
            ChunkType::NoHeader => 0,
        };

        if buf.len() < basic_header_len + msg_header_len {
            return Ok(None);
        }

        // check if extended
        let has_extended_ts = if msg_header_len >= 3 {
            let offset = basic_header_len;
            let b0 = buf[offset];
            let b1 = buf[offset + 1];
            let b2 = buf[offset + 2];
            b0 == 0xFF && b1 == 0xFF && b2 == 0xFF
        } else if fmt == ChunkType::NoHeader {
            if let Some(prev) = self.prev_headers.get(&cs_id) {
                prev.timestamp_delta == 0xFFFFFF || prev.timestamp == 0xFFFFFF
            } else {
                false
            }
        } else {
            false
        };

        let extended_ts_len = if has_extended_ts { 4 } else { 0 };

        if buf.len() < basic_header_len + msg_header_len + extended_ts_len {
            return Ok(None);
        }

        // chunk payload size
        let msg_len = if fmt == ChunkType::Full || fmt == ChunkType::NoMessageStreamId {
            let offset = basic_header_len + 3; // skip timestamp
            let b0 = buf[offset] as u32;
            let b1 = buf[offset + 1] as u32;
            let b2 = buf[offset + 2] as u32;
            (b0 << 16) | (b1 << 8) | b2
        } else {
            self.prev_headers
                .get(&cs_id)
                .map(|h| h.msg_len)
                .unwrap_or(0)
        };

        if msg_len as usize > MAX_MESSAGE_SIZE {
            return Err(RtmpError::MessageTooLarge(msg_len));
        }

        let current_acc = accumulators
            .get(&cs_id)
            .map(|a| a.current_len())
            .unwrap_or(0);

        let remaining_for_message = (msg_len as usize).saturating_sub(current_acc);
        let chunk_payload_size = min(remaining_for_message, self.chunk_size);

        Ok(Some(
            basic_header_len + msg_header_len + extended_ts_len + chunk_payload_size,
        ))
    }

    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }

    fn parse_chunk_data(
        &mut self,
        data: Vec<u8>,
        accumulators: &HashMap<u32, PayloadAccumulator>,
    ) -> Result<RtmpChunk, RtmpError> {
        let mut cursor = 0;

        // basic header
        let first_byte = data[cursor];
        cursor += 1;

        let fmt = ChunkType::from(first_byte >> 6);
        let cs_id_marker = first_byte & 0x3F;

        let cs_id = match cs_id_marker {
            0 => {
                let b = data[cursor];
                cursor += 1;
                64 + (b as u32)
            }
            1 => {
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
                // [timestamp(3)] [msg_len(3)] [msg_type_id(1)] [msg_stream_id(4 LE)]
                let ts_raw = read_u24(&data[cursor..]);
                msg_len = read_u24(&data[cursor + 3..]);
                msg_type_id = data[cursor + 6];
                msg_stream_id = read_u32_le(&data[cursor + 7..]);
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
                // [timestamp_delta(3)] [msg_len(3)] [msg_type_id(1)]
                let ts_delta_raw = read_u24(&data[cursor..]);
                msg_len = read_u24(&data[cursor + 3..]);
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
                // [timestamp_delta(3)]
                let ts_delta_raw = read_u24(&data[cursor..]);
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
            let extended_ts = read_u32_be(&data[cursor..]);
            cursor += 4;

            match fmt {
                ChunkType::Full => timestamp = extended_ts,
                _ => timestamp_delta = extended_ts,
            }
        }

        // calculate timestamp
        if fmt != ChunkType::Full {
            timestamp = timestamp.wrapping_add(timestamp_delta);
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

        // paylaod
        let current_acc = accumulators
            .get(&cs_id)
            .map(|a| a.current_len())
            .unwrap_or(0);

        let remaining_for_message = (msg_len as usize).saturating_sub(current_acc);
        let chunk_payload_size = min(remaining_for_message, self.chunk_size);

        let payload_bytes = &data[cursor..cursor + chunk_payload_size];
        let payload = Bytes::copy_from_slice(payload_bytes);

        Ok(RtmpChunk { header, payload })
    }
}

fn read_u24(s: &[u8]) -> u32 {
    let b0 = s[0] as u32;
    let b1 = s[1] as u32;
    let b2 = s[2] as u32;
    (b0 << 16) | (b1 << 8) | b2
}

fn read_u32_be(s: &[u8]) -> u32 {
    let b0 = s[0] as u32;
    let b1 = s[1] as u32;
    let b2 = s[2] as u32;
    let b3 = s[3] as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

fn read_u32_le(s: &[u8]) -> u32 {
    let b0 = s[0] as u32;
    let b1 = s[1] as u32;
    let b2 = s[2] as u32;
    let b3 = s[3] as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}
