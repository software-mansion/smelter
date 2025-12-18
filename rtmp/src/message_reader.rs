use crate::chunk::{ChunkHeader, ChunkType};
use crate::error::RtmpError;
use crate::message::RtmpMessage;
use bytes::BytesMut;
use std::cmp::min;
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::net::TcpStream;

const MAX_MESSAGE_SIZE: usize = 5 * 1024 * 1024; // 5 MB

#[allow(unused)]
struct PayloadAccumulator {
    length: usize,
    buffer: BytesMut,
}

pub struct RtmpMessageReader {
    stream: TcpStream,
    prev_headers: HashMap<u32, ChunkHeader>,
    partial_payloads: HashMap<u32, PayloadAccumulator>,
    chunk_size: usize,
}

impl RtmpMessageReader {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            prev_headers: HashMap::new(),
            partial_payloads: HashMap::new(),
            chunk_size: 128, // Default RTMP chunk size
        }
    }

    // https://rtmp.veriskope.com/docs/spec/#5311-chunk-basic-header
    fn read_basic_header(&mut self) -> Result<(ChunkType, u32), RtmpError> {
        let byte = self.read_u8()?;
        let fmt = ChunkType::from(byte >> 6);
        let cs_id_initial = byte & 0x3F;

        let cs_id = match cs_id_initial {
            0 => {
                let next_byte = self.read_u8()?;
                64 + (next_byte as u32)
            }
            1 => {
                let second_byte = self.read_u8()?;
                let third_byte = self.read_u8()?;
                64 + (second_byte as u32) + ((third_byte as u32) * 256)
            }
            n => n as u32,
        };

        Ok((fmt, cs_id))
    }

    // https://rtmp.veriskope.com/docs/spec/#5312-chunk-message-header
    fn read_message_header(
        &mut self,
        fmt: ChunkType,
        cs_id: u32,
    ) -> Result<ChunkHeader, RtmpError> {
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

        match fmt {
            ChunkType::Full => {
                // type 0 (11 bytes)
                timestamp = self.read_u24()?;
                msg_len = self.read_u24()?;
                msg_type_id = self.read_u8()?;
                msg_stream_id = self.read_u32_le()?;
                timestamp_delta = 0;
            }
            ChunkType::NoMessageStreamId => {
                // type 1 (7 bytes)
                timestamp_delta = self.read_u24()?;
                msg_len = self.read_u24()?;
                msg_type_id = self.read_u8()?;
                // reuse msg_stream_id from prevous chunk header
            }
            ChunkType::TimestampOnly => {
                // type 2 (3 bytes)
                timestamp_delta = self.read_u24()?;
                // reuse msg_len, msg_type_id, msg_stream_id
            }
            ChunkType::NoHeader => {
                // type 3
                // reuse everything
            }
        }

        // https://rtmp.veriskope.com/docs/spec/#5313-extended-timestamp
        if fmt == ChunkType::Full {
            if timestamp == 0xFFFFFF {
                let extended = self.read_u32()?;
                timestamp = extended;
            }
        } else {
            if timestamp_delta == 0xFFFFFF {
                let extended = self.read_u32()?;
                timestamp_delta = extended;
            }

            timestamp = timestamp.wrapping_add(timestamp_delta);
        };

        Ok(ChunkHeader {
            fmt,
            cs_id,
            timestamp,
            timestamp_delta,
            msg_len,
            msg_type_id,
            msg_stream_id,
        })
    }

    fn read_u8(&mut self) -> Result<u8, RtmpError> {
        let mut buf = [0u8; 1];
        self.stream.read_exact(&mut buf)?;
        Ok(buf[0])
    }
    fn read_u24(&mut self) -> Result<u32, RtmpError> {
        let mut buf = [0u8; 4];
        self.stream.read_exact(&mut buf[1..])?;
        Ok(u32::from_be_bytes(buf))
    }
    fn read_u32(&mut self) -> Result<u32, RtmpError> {
        let mut buf = [0u8; 4];
        self.stream.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }
    fn read_u32_le(&mut self) -> Result<u32, RtmpError> {
        let mut buf = [0u8; 4];
        self.stream.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }
}

impl Iterator for RtmpMessageReader {
    type Item = Result<RtmpMessage, RtmpError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (fmt, cs_id) = match self.read_basic_header() {
                Ok(val) => val,
                Err(e) => {
                    if let RtmpError::Io(io_err) = &e
                        && io_err.kind() == ErrorKind::UnexpectedEof
                    {
                        return None;
                    }
                    return Some(Err(e));
                }
            };

            let header = match self.read_message_header(fmt, cs_id) {
                Ok(h) => h,
                Err(e) => return Some(Err(e)),
            };

            self.prev_headers.insert(cs_id, header.clone());

            if header.msg_len as usize > MAX_MESSAGE_SIZE {
                return Some(Err(RtmpError::MessageTooLarge(header.msg_len)));
            }

            let accumulator = self.partial_payloads.entry(cs_id).or_insert_with(|| {
                let initial_cap = min(header.msg_len as usize, 4096);
                PayloadAccumulator {
                    length: header.msg_len as usize,
                    buffer: BytesMut::with_capacity(initial_cap),
                }
            });

            let current_len = accumulator.buffer.len();
            let remaining = accumulator.length - current_len;
            let to_read = min(remaining, self.chunk_size);

            let mut chunk_buf = vec![0u8; to_read];
            if let Err(err) = self.stream.read_exact(&mut chunk_buf) {
                return Some(Err(RtmpError::from(err)));
            }
            accumulator.buffer.extend_from_slice(&chunk_buf);

            if accumulator.buffer.len() == accumulator.length
                && let Some(finished_acc) = self.partial_payloads.remove(&cs_id)
            {
                return Some(Ok(RtmpMessage {
                    timestamp: header.timestamp,
                    type_id: header.msg_type_id,
                    stream_id: header.msg_stream_id,
                    payload: finished_acc.buffer.freeze(),
                }));
            }
        }
    }
}
