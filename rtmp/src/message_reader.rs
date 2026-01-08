use crate::chunk::{ChunkType, RtmpChunk, RtmpChunkReader};
use crate::error::RtmpError;
use crate::message::RtmpMessage;
use bytes::BytesMut;
use std::cmp::min;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::{Arc, atomic::AtomicBool};

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

    #[allow(unused)]
    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_reader.set_chunk_size(size);
    }

    fn accumulate_chunk(&mut self, chunk: &RtmpChunk) -> Result<(), RtmpError> {
        let cs_id = chunk.header.cs_id;
        let is_new_message = chunk.header.fmt != ChunkType::NoHeader;

        if is_new_message {
            self.accumulators.insert(
                cs_id,
                PayloadAccumulator::new(chunk.header.msg_len as usize),
            );
        }

        let acc = self
            .accumulators
            .get_mut(&cs_id)
            .ok_or(RtmpError::MissingHeader(cs_id))?;

        acc.append(&chunk.payload);
        Ok(())
    }

    fn try_complete_message(&mut self, chunk: &RtmpChunk) -> Option<RtmpMessage> {
        let cs_id = chunk.header.cs_id;
        let acc = self.accumulators.get(&cs_id)?;
        if acc.buffer.len() < acc.expected_length {
            return None;
        }
        let acc = self.accumulators.remove(&cs_id)?;
        Some(RtmpMessage {
            timestamp: chunk.header.timestamp,
            type_id: chunk.header.msg_type_id,
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
                Err(e) => return Some(Err(e)),
            };
            if let Err(e) = self.accumulate_chunk(&chunk) {
                return Some(Err(e));
            }
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
