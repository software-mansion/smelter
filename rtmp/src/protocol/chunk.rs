use std::collections::VecDeque;

use crate::{ParseError, RtmpError};

const MAX_MESSAGE_SIZE: usize = 5 * 1024 * 1024; // 5 MB

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
        if data.len() < 1 {
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

    pub fn msg_len(&self) -> Option<u32> {
        match self {
            Self::Full { msg_len, .. } => Some(*msg_len),
            Self::NoMessageStreamId { msg_len, .. } => Some(*msg_len),
            Self::TimestampOnly { .. } | Self::NoHeader => None,
        }
    }

    pub fn msg_timestamp(&self) -> Option<u32> {
        match self {
            Self::Full { timestamp, .. } => Some(*timestamp),
            Self::NoMessageStreamId {
                timestamp_delta, ..
            } => Some(*timestamp_delta),
            Self::TimestampOnly { timestamp_delta } => Some(*timestamp_delta),
            Self::NoHeader => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkExtendedTimestamp {
    timestamp: u32,
}

impl ChunkExtendedTimestamp {
    pub fn try_read(data: &VecDeque<u8>, offset: usize) -> Result<(Self, usize), ParseChunkError> {
        if data.len() < offset + 4 {
            return Err(ParseChunkError::NotEnoughData);
        }
        Ok((
            Self {
                timestamp: read_u32_be(data, offset),
            },
            offset + 4,
        ))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct VirtualMessageHeader {
    pub msg_len: u32,
    pub msg_type_id: u8,
    pub msg_stream_id: u32,
    pub timestamp: u32,
}

impl VirtualMessageHeader {
    pub fn from_msg(
        prev: Option<Self>,
        msg_header: ChunkMessageHeader,
    ) -> Result<Self, ParseChunkError> {
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
                timestamp,
            }),
            (
                ChunkMessageHeader::NoMessageStreamId {
                    timestamp_delta,
                    msg_len,
                    msg_type_id,
                },
                Some(prev),
            ) => {
                prev.timestamp += timestamp_delta;
                prev.msg_len = msg_len;
                prev.msg_type_id = msg_type_id;
                Ok(prev)
            }
            (ChunkMessageHeader::TimestampOnly { timestamp_delta }, Some(prev)) => {
                prev.timestamp += timestamp_delta;
                Ok(prev)
            }
            (ChunkMessageHeader::NoHeader, Some(prev)) => Ok(prev),
            (_, None) => {
                Err(RtmpError::ParsingError(ParseError::NotEnoughData)) // TODO: better error
            }
        }
    }
}

impl RtmpChunkReader {
    //    pub fn new(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
    //        Self {
    //            reader: BufferedReader::new(socket, should_close),
    //            context: HashMap::new(),
    //            chunk_size: DEFAULT_CHUNK_SIZE,
    //        }
    //    }
    //
    //    pub fn read_chunk(&mut self) -> Result<RawMessage, RtmpError> {
    //        loop {
    //            match self.try_parse_chunk() {
    //                Ok((chunk, len_to_read)) => {
    //                    self.reader.read_bytes(len_to_read)?;
    //                    return Ok(chunk);
    //                }
    //                Err(ParseChunkError::NotEnoughData) => {
    //                    let current_len = self.reader.data().len();
    //                    self.reader.read_until_buffer_size(current_len + 1)?;
    //                }
    //                Err(ParseChunkError::RtmpError(e)) => return Err(e),
    //            }
    //        }
    //    }
    //
    //    pub fn set_chunk_size(&mut self, size: usize) {
    //        self.chunk_size = size;
    //    }

    //    fn try_parse_chunk(&mut self) -> Result<RawMessage, ParseChunkError> {
    //        let data = self.reader.data();
    //
    //        if data.is_empty() {
    //            return Err(ParseChunkError::NotEnoughData);
    //        }
    //
    //        // basic header 1, 2 or 3 bytes
    //        let fmt = ChunkType::from((data[1] & 0b1100_0000) >> 6);
    //        let (cs_id, offset) = self.try_read_cs_id(&data)?;
    //
    //        let (msg_header, offset) = self.try_read_msg_header(fmt, &data, offset)?;
    //
    //        match msg_header {
    //            MessageHeader::Full {
    //                timestamp,
    //                msg_len,
    //                msg_type_id,
    //                msg_stream_id,
    //            } => self.context.entry(cs_id).or_insert_with(Default::default),
    //            _ => {}
    //        }
    //        let context = self.context.entry(cs_id).or_insert_with(Default::default);
    //        let (extended_timestamp, offset) = match msg_header.has_extended_timestamp(&context) {
    //            true => {
    //                if data.len() < offset + 4 {
    //                    return Err(ParseChunkError::NotEnoughData);
    //                }
    //                (Some(read_u32_be(data, offset)), offset + 4)
    //            }
    //            false => (None, offset),
    //        };
    //
    //        let msg_len = msg_header.msg_len(context)?;
    //
    //        if msg_len as usize > MAX_MESSAGE_SIZE {
    //            return Err(RtmpError::MessageTooLarge(msg_len).into());
    //        }
    //
    //        let payload_size = (msg_len as usize)
    //            .saturating_sub(context.payload_acc.len())
    //            .min(self.chunk_size);
    //
    //        if data.len() < offset + payload_size {
    //            return Err(ParseChunkError::NotEnoughData);
    //        }
    //
    //        context.update(msg_header, extended_timestamp)?;
    //
    //        let payload = Bytes::from_iter(data.iter().skip(offset).take(payload_size).copied());
    //
    //        let chunk = Chunk {
    //            fmt,
    //            cs_id,
    //            msg_header,
    //            timestamp: context.timestamp.unwrap(),
    //            payload,
    //        };
    //
    //        self.reader.read_bytes(offset + payload_size)?;
    //
    //        if let Some(msg_payload) = context.payload_acc.try_pop()? {
    //            Ok(RawMessage {
    //                timestamp: context.timestamp,
    //                msg_type: MessageType::try_from_raw(context.msg_type_id)?,
    //                stream_id: context.msg_stream_id,
    //                payload: acc.buffer.freeze(),
    //            })
    //        } else {
    //            Ok(Err(ParseChunkError::NotEnoughData))
    //        }
    //    }
}

//impl ChunkMessageHeader {
//    fn has_extended_timestamp(&self, context: &ChunkStreamContext) -> bool {
//        match self {
//            MessageHeader::Full { timestamp, .. } => *timestamp == 0xFFFFFF,
//            MessageHeader::NoMessageStreamId {
//                timestamp_delta, ..
//            } => *timestamp_delta == 0xFFFFFF,
//            MessageHeader::TimestampOnly { timestamp_delta } => *timestamp_delta == 0xFFFFFF,
//            MessageHeader::NoHeader => match context.timestamp {
//                Some(prev) => prev == 0xFFFFFF,
//                None => false,
//            },
//        }
//    }
//
//    fn timestamp(&self) -> Option<u32> {
//        match self {
//            MessageHeader::Full { timestamp, .. } => Some(*timestamp),
//            MessageHeader::NoMessageStreamId {
//                timestamp_delta, ..
//            } => Some(*timestamp_delta),
//            MessageHeader::TimestampOnly { timestamp_delta } => Some(*timestamp_delta),
//            MessageHeader::NoHeader => None,
//        }
//    }
//
//    pub fn msg_len(&self, context: &ChunkStreamContext) -> Result<u32, ParseChunkError> {
//        match self {
//            MessageHeader::Full { msg_len, .. } => Ok(*msg_len),
//            MessageHeader::NoMessageStreamId { msg_len, .. } => Ok(*msg_len),
//            MessageHeader::TimestampOnly { .. } | MessageHeader::NoHeader => {
//                match context.msg_len {
//                    Some(_) => todo!(),
//                    None => Err(RtmpError::InternalError("missing msg_len")),
//                }
//            }
//        }
//    }
//}
//
//impl ChunkStreamContext {
//    fn update(
//        &mut self,
//        header: MessageHeader,
//        extended_timestamp: Option<u32>,
//    ) -> Result<(), ParseChunkError> {
//        match header {
//            MessageHeader::Full {
//                timestamp,
//                msg_len,
//                msg_type_id,
//                msg_stream_id,
//            } => {
//                self.timestamp = timestamp;
//                self.msg_len = msg_len;
//                self.msg_type_id = msg_type_id;
//                self.msg_stream_id = msg_stream_id;
//            }
//            MessageHeader::NoMessageStreamId {
//                timestamp_delta,
//                msg_len,
//                msg_type_id,
//            } => {
//                let timestamp_delta = extended_timestamp.unwrap_or(timestamp_delta);
//                self.timestamp = ts.wrapping_add(timestamp_delta);
//                self.msg_len = msg_len;
//                self.msg_type_id = msg_type_id;
//            }
//            MessageHeader::TimestampOnly { timestamp_delta } => {
//                let Some(ts) = self.timestamp else {
//                    return Err(RtmpError::InternalError("missing timestamp").into());
//                };
//                let timestamp_delta = extended_timestamp.unwrap_or(timestamp_delta);
//                self.timestamp = Some(ts.wrapping_add(timestamp_delta));
//            }
//            MessageHeader::NoHeader => (),
//        };
//        Ok(())
//    }
//}

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
