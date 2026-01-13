use crate::{
    chunk::{ChunkType, RtmpChunk, RtmpChunkReader},
    error::RtmpError,
    message::RtmpMessage,
    protocol::MessageType,
};
use bytes::BytesMut;
use std::net::TcpStream;
use std::{
    cmp::min,
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

pub struct RtmpMessageReader {
    chunk_reader: RtmpChunkReader,
    accumulators: HashMap<u32, PayloadAccumulator>,
}

impl RtmpMessageReader {
    pub fn new(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        Self {
            chunk_reader: RtmpChunkReader::new(socket, should_close),
            accumulators: HashMap::new(),
        }
    }

    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_reader.set_chunk_size(size);
    }

    fn accumulate_chunk(&mut self, chunk: &RtmpChunk) {
        let cs_id = chunk.header.cs_id;

        match chunk.header.fmt {
            ChunkType::Full | ChunkType::NoMessageStreamId => {
                // types 0 and 1 start new message
                self.accumulators.insert(
                    cs_id,
                    PayloadAccumulator::new(chunk.header.msg_len as usize),
                );
            }
            ChunkType::TimestampOnly | ChunkType::NoHeader => {
                // types 2 and 3 continue existing message or start new with inherited message length
                self.accumulators
                    .entry(cs_id)
                    .or_insert_with(|| PayloadAccumulator::new(chunk.header.msg_len as usize));
            }
        }

        let acc = self.accumulators.get_mut(&cs_id).unwrap();
        acc.append(&chunk.payload);
    }

    fn try_complete_message(&mut self, chunk: &RtmpChunk) -> Option<RtmpMessage> {
        let cs_id = chunk.header.cs_id;
        let acc = self.accumulators.get(&cs_id)?;
        if acc.buffer.len() < acc.expected_length {
            return None;
        }
        let acc = self.accumulators.remove(&cs_id)?;
        let msg_type = MessageType::try_from_id(chunk.header.msg_type_id).ok()?;
        Some(RtmpMessage {
            timestamp: chunk.header.timestamp,
            msg_type,
            stream_id: chunk.header.msg_stream_id,
            payload: acc.buffer.freeze(),
        })
    }
}

impl Iterator for RtmpMessageReader {
    type Item = Result<RtmpMessage, RtmpError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let chunk = match self.chunk_reader.read_chunk(&self.accumulators) {
                Ok(chunk) => chunk,
                Err(RtmpError::UnexpectedEof) => return None,
                Err(RtmpError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                    return None;
                }
                Err(RtmpError::Io(e)) if e.kind() == std::io::ErrorKind::BrokenPipe => return None,
                Err(e) => return Some(Err(e)),
            };
            self.accumulate_chunk(&chunk);
            if let Some(msg) = self.try_complete_message(&chunk) {
                return Some(Ok(msg));
            }
        }
    }
}

pub struct PayloadAccumulator {
    expected_length: usize,
    buffer: BytesMut,
}

impl PayloadAccumulator {
    pub(crate) fn new(expected_length: usize) -> Self {
        let initial_cap = min(expected_length, 4096);
        Self {
            expected_length,
            buffer: BytesMut::with_capacity(initial_cap),
        }
    }

    pub(crate) fn append(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub(crate) fn current_len(&self) -> usize {
        self.buffer.len()
    }
}
