use crate::chunk::{ChunkHeader, ChunkType};
use crate::error::RtmpError;
use crate::message::RtmpMessage;
use bytes::BytesMut;
use std::cmp::min;
use std::collections::{HashMap, VecDeque};
use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

const MAX_MESSAGE_SIZE: usize = 5 * 1024 * 1024; // 5 MB

#[allow(unused)]
struct PayloadAccumulator {
    length: usize,
    buffer: BytesMut,
}

pub struct RtmpMessageReader {
    stream: TcpStream,
    buf: VecDeque<u8>,
    read_buf: Vec<u8>,
    should_close: Arc<AtomicBool>,

    prev_headers: HashMap<u32, ChunkHeader>,
    partial_payloads: HashMap<u32, PayloadAccumulator>,
    chunk_size: usize,
}

impl RtmpMessageReader {
    pub fn new(stream: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        stream
            .set_nonblocking(false)
            .expect("Cannot set blocking tcp input stream");
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("Cannot set read timeout");

        Self {
            stream,
            buf: VecDeque::new(),
            read_buf: vec![0; 65536],
            should_close,
            prev_headers: HashMap::new(),
            partial_payloads: HashMap::new(),
            chunk_size: 128, // Default RTMP chunk size
        }
    }

    // https://rtmp.veriskope.com/docs/spec/#5311-chunk-basic-header
    fn read_basic_header(&mut self) -> Result<(ChunkType, u32), RtmpError> {
        self.read_until_buffer_size(1)?;

        let first_byte = self.buf[0];
        let cs_id_initial = first_byte & 0x3F;

        let header_len = match cs_id_initial {
            0 => 2,
            1 => 3,
            _ => 1,
        };

        self.read_until_buffer_size(header_len)?;

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
        let header_size = match fmt {
            ChunkType::Full => 11,
            ChunkType::NoMessageStreamId => 7,
            ChunkType::TimestampOnly => 3,
            ChunkType::NoHeader => 0,
        };

        self.read_until_buffer_size(header_size)?;

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

        let has_extended = match fmt {
            ChunkType::NoHeader => timestamp_delta >= 0xFFFFFF,
            _ => {
                let mut temp = [0u8; 4];
                temp[1] = self.buf[0];
                temp[2] = self.buf[1];
                temp[3] = self.buf[2];

                let timestamp = u32::from_be_bytes(temp);
                timestamp >= 0xFFFFFF
            }
        };

        let extended_len = if has_extended { 4 } else { 0 };
        let total_len = header_size + extended_len;

        self.read_until_buffer_size(total_len)?;

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
            if timestamp >= 0xFFFFFF {
                let extended = self.read_u32()?;
                timestamp = extended;
            }
        } else {
            if timestamp_delta >= 0xFFFFFF {
                let extended = self.read_u32()?;
                timestamp_delta = extended;
            }

            timestamp = timestamp.wrapping_add(timestamp_delta);
        }

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

    fn read_until_buffer_size(&mut self, buffer_size: usize) -> Result<(), RtmpError> {
        loop {
            if self.buf.len() >= buffer_size {
                return Ok(());
            }
            match self.stream.read(&mut self.read_buf) {
                Ok(0) => return Err(RtmpError::UnexpectedEof),
                Ok(read_bytes) => {
                    self.buf.extend(self.read_buf[0..read_bytes].iter());
                }
                Err(err) => {
                    let should_close = self.should_close.load(std::sync::atomic::Ordering::Relaxed);
                    match err.kind() {
                        std::io::ErrorKind::WouldBlock if !should_close => {
                            continue;
                        }
                        std::io::ErrorKind::WouldBlock => return Err(err.into()),
                        _ => {
                            return Err(err.into());
                        }
                    }
                }
            };
        }
    }

    fn read_exact_bytes(&mut self, len: usize) -> Result<Vec<u8>, RtmpError> {
        self.read_until_buffer_size(len)?;

        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(self.buf.pop_front().unwrap());
        }
        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8, RtmpError> {
        let mut buf = [0u8; 1];
        self.buf.read_exact(&mut buf)?;
        Ok(buf[0])
    }
    fn read_u24(&mut self) -> Result<u32, RtmpError> {
        let mut buf = [0u8; 4];
        self.buf.read_exact(&mut buf[1..])?;
        Ok(u32::from_be_bytes(buf))
    }
    fn read_u32(&mut self) -> Result<u32, RtmpError> {
        let mut buf = [0u8; 4];
        self.buf.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }
    fn read_u32_le(&mut self) -> Result<u32, RtmpError> {
        let mut buf = [0u8; 4];
        self.buf.read_exact(&mut buf)?;
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
            if self.should_close.load(Ordering::Relaxed) {
                return None;
            }

            let (fmt, cs_id) = match self.read_basic_header() {
                Ok(val) => val,
                Err(e) => {
                    if let RtmpError::Io(io_err) = &e
                        && (io_err.kind() == ErrorKind::UnexpectedEof)
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

            let (current_len, total_len) = {
                let acc = self.partial_payloads.entry(cs_id).or_insert_with(|| {
                    let initial_cap = min(header.msg_len as usize, 4096);
                    PayloadAccumulator {
                        length: header.msg_len as usize,
                        buffer: BytesMut::with_capacity(initial_cap),
                    }
                });
                (acc.buffer.len(), acc.length)
            };

            let remaining = total_len - current_len;
            let to_read = min(remaining, self.chunk_size);

            let bytes = match self.read_exact_bytes(to_read) {
                Ok(bytes) => bytes,
                Err(e) => return Some(Err(e)),
            };

            let is_finished = {
                let accumulator = self.partial_payloads.get_mut(&cs_id).unwrap();
                accumulator.buffer.extend_from_slice(&bytes);
                accumulator.buffer.len() == accumulator.length
            };

            if is_finished && let Some(finished_acc) = self.partial_payloads.remove(&cs_id) {
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
