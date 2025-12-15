use bytes::{Buf, BufMut, Bytes, BytesMut};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ChunkMessageHeader {
    pub chunk_stream_id: u32,
    pub timestamp: u32,
    pub msg_length: u32,
    pub msg_type_id: u8,
    pub msg_stream_id: u32,
}

#[derive(Error, Debug)]
pub enum ChunkMessageHeaderError {
    #[error("Insufficient data")]
    InsufficientData,
    #[error("Invalid header format")]
    InvalidFormat,
}

impl ChunkMessageHeader {
    pub fn new(chunk_stream_id: u32, msg_stream_id: u32, timestamp: u32) -> Self {
        Self {
            chunk_stream_id,
            timestamp,
            msg_length: 0,
            msg_type_id: 0,
            msg_stream_id,
        }
    }
    pub fn parse(
        mut buf: Bytes,
        prev_header: Option<&ChunkMessageHeader>,
    ) -> Result<(ChunkMessageHeader, Bytes), ChunkMessageHeaderError> {
        if !buf.has_remaining() {
            return Err(ChunkMessageHeaderError::InsufficientData);
        }

        let first_byte = buf.get_u8();
        let fmt = (first_byte >> 6) & 0x03;
        let cs_id = first_byte & 0x3F;

        let chunk_stream_id = match cs_id {
            0 => {
                if buf.remaining() < 1 {
                    return Err(ChunkMessageHeaderError::InsufficientData);
                }
                buf.get_u8() as u32 + 64
            }
            1 => {
                if buf.remaining() < 2 {
                    return Err(ChunkMessageHeaderError::InsufficientData);
                }
                let b1 = buf.get_u8() as u32;
                let b2 = buf.get_u8() as u32;
                b2 * 256 + b1 + 64
            }
            _ => cs_id as u32,
        };

        let header = match fmt {
            0 => {
                // type 0: full header
                if buf.remaining() < 11 {
                    return Err(ChunkMessageHeaderError::InsufficientData);
                }
                let timestamp = read_u24(&mut buf);
                let msg_length = read_u24(&mut buf);
                let msg_type_id = buf.get_u8();
                let msg_stream_id = buf.get_u32_le();

                let timestamp = if timestamp == 0xFFFFFF {
                    if buf.remaining() < 4 {
                        return Err(ChunkMessageHeaderError::InsufficientData);
                    }
                    buf.get_u32()
                } else {
                    timestamp
                };

                ChunkMessageHeader {
                    chunk_stream_id,
                    timestamp,
                    msg_length,
                    msg_type_id,
                    msg_stream_id,
                }
            }
            1 => {
                // type 1: same stream ID
                if buf.remaining() < 7 {
                    return Err(ChunkMessageHeaderError::InsufficientData);
                }
                let prev = prev_header.ok_or(ChunkMessageHeaderError::InvalidFormat)?;
                let timestamp_delta = read_u24(&mut buf);
                let msg_length = read_u24(&mut buf);
                let msg_type_id = buf.get_u8();

                let timestamp_delta = if timestamp_delta == 0xFFFFFF {
                    if buf.remaining() < 4 {
                        return Err(ChunkMessageHeaderError::InsufficientData);
                    }
                    buf.get_u32()
                } else {
                    timestamp_delta
                };

                ChunkMessageHeader {
                    chunk_stream_id,
                    timestamp: prev.timestamp.wrapping_add(timestamp_delta),
                    msg_length,
                    msg_type_id,
                    msg_stream_id: prev.msg_stream_id,
                }
            }
            2 => {
                // type 2: same stream ID, length, type
                if buf.remaining() < 3 {
                    return Err(ChunkMessageHeaderError::InsufficientData);
                }
                let prev = prev_header.ok_or(ChunkMessageHeaderError::InvalidFormat)?;
                let timestamp_delta = read_u24(&mut buf);

                let timestamp_delta = if timestamp_delta == 0xFFFFFF {
                    if buf.remaining() < 4 {
                        return Err(ChunkMessageHeaderError::InsufficientData);
                    }
                    buf.get_u32()
                } else {
                    timestamp_delta
                };

                ChunkMessageHeader {
                    chunk_stream_id,
                    timestamp: prev.timestamp.wrapping_add(timestamp_delta),
                    msg_length: prev.msg_length,
                    msg_type_id: prev.msg_type_id,
                    msg_stream_id: prev.msg_stream_id,
                }
            }
            3 => {
                // type 3: no header
                let prev = prev_header.ok_or(ChunkMessageHeaderError::InvalidFormat)?;
                ChunkMessageHeader {
                    chunk_stream_id,
                    timestamp: prev.timestamp,
                    msg_length: prev.msg_length,
                    msg_type_id: prev.msg_type_id,
                    msg_stream_id: prev.msg_stream_id,
                }
            }
            _ => unreachable!(),
        };

        Ok((header, buf))
    }

    pub fn encode(&self, fmt: u8) -> Vec<u8> {
        let mut buf = BytesMut::new();

        let first_byte = (fmt << 6) | (self.chunk_stream_id as u8 & 0x3F);
        buf.put_u8(first_byte);

        match fmt {
            0 => {
                write_u24(&mut buf, self.timestamp.min(0xFFFFFF));
                write_u24(&mut buf, self.msg_length);
                buf.put_u8(self.msg_type_id);
                buf.put_u32_le(self.msg_stream_id);
                if self.timestamp >= 0xFFFFFF {
                    buf.put_u32(self.timestamp);
                }
            }
            1 => {
                write_u24(&mut buf, self.timestamp.min(0xFFFFFF));
                write_u24(&mut buf, self.msg_length);
                buf.put_u8(self.msg_type_id);
                if self.timestamp >= 0xFFFFFF {
                    buf.put_u32(self.timestamp);
                }
            }
            2 => {
                write_u24(&mut buf, self.timestamp.min(0xFFFFFF));
                if self.timestamp >= 0xFFFFFF {
                    buf.put_u32(self.timestamp);
                }
            }
            3 => {}
            _ => unreachable!(),
        }

        buf.to_vec()
    }
}

fn read_u24(buf: &mut Bytes) -> u32 {
    let b1 = buf.get_u8() as u32;
    let b2 = buf.get_u8() as u32;
    let b3 = buf.get_u8() as u32;
    (b1 << 16) | (b2 << 8) | b3
}

fn write_u24(buf: &mut BytesMut, value: u32) {
    buf.put_u8((value >> 16) as u8);
    buf.put_u8((value >> 8) as u8);
    buf.put_u8(value as u8);
}
