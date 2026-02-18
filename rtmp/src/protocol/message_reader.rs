use std::{
    cmp::min,
    collections::{HashMap, VecDeque},
    io::ErrorKind,
    net::TcpStream,
    sync::{Arc, atomic::AtomicBool},
};

use bytes::{Bytes, BytesMut};

use crate::{
    error::RtmpError,
    message::RtmpMessage,
    protocol::{
        MessageType, RawMessage,
        chunk::{Chunk, RtmpChunkReader},
    },
};

pub(crate) struct RtmpMessageReader {
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

    fn try_complete_message(&mut self, chunk: &Chunk) -> Result<Option<RawMessage>, RtmpError> {
        let cs_id = chunk.cs_id;

        let acc = self
            .accumulators
            .entry(cs_id)
            .or_insert_with(|| PayloadAccumulator::new());

        match chunk.msg_header {
            MessageHeader::Full {
                timestamp,
                msg_len,
                msg_type_id,
                msg_stream_id,
            } => todo!(),
            MessageHeader::NoMessageStreamId {
                timestamp_delta,
                msg_len,
                msg_type_id,
            } => todo!(),
            MessageHeader::TimestampOnly { timestamp_delta } => todo!(),
            MessageHeader::NoHeader => todo!(),
        };

        let acc = self.accumulators.get_mut(&cs_id).unwrap();
        acc.append(chunk.payload);
        if let Some(message_payload) = acc.try_pop()? {
            let msg_type = MessageType::try_from_raw(chunk.header.msg_type_id)?;
            Ok(Some(RawMessage {
                timestamp: chunk.timestamp,
                msg_type,
                stream_id: chunk.msg_stream_id,
                payload: acc.buffer.freeze(),
            }))
        } else {
            Ok(None)
        }
    }
}

impl Iterator for RtmpMessageReader {
    type Item = Result<RtmpMessage, RtmpError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let chunk = match self.chunk_reader.read_chunk(&self.accumulators) {
                Ok(chunk) => chunk,
                Err(RtmpError::UnexpectedEof) => return None,
                Err(RtmpError::Io(e)) if e.kind() == ErrorKind::ConnectionReset => {
                    return None;
                }
                Err(RtmpError::Io(e)) if e.kind() == ErrorKind::BrokenPipe => return None,
                Err(e) => return Some(Err(e)),
            };
            if let Some(msg) = self.try_complete_message(chunk) {
                return match RtmpMessage::from_raw(msg) {
                    Ok(msg) => Some(Ok(msg)),
                    Err(err) => Some(Err(err.into())),
                };
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct PayloadAccumulator {
    current_message_length: usize,
    buffer: VecDeque<Bytes>,
}

impl PayloadAccumulator {
    pub fn append(&mut self, data: Bytes) {
        self.buffer.push_back(data);
    }

    pub fn try_pop(&mut self) -> Result<Option<Bytes>, RtmpError> {
        let current_buffer_size = self.len();
        if self.current_message_length == 0 {
            Err(RtmpError::InternalError(""))
        } else if self.current_message_length == current_buffer_size {
            Ok(Some(Bytes::from_iter(
                std::mem::take(&mut self.buffer).iter().flatten().copied(),
            )))
        } else if self.current_message_length >= current_buffer_size {
            Err(RtmpError::InternalError("Message to long"))
        } else {
            Ok(None)
        }
    }
    pub fn len(&self) -> usize {
        self.buffer.iter().map(|p| p.len()).sum()
    }
}
