use std::{
    collections::{HashMap, VecDeque},
    io::ErrorKind,
    net::TcpStream,
    sync::{Arc, atomic::AtomicBool},
};

use bytes::Bytes;
use tracing::warn;

use crate::{
    error::RtmpError,
    protocol::{
        RawMessage,
        buffered_stream_reader::BufferedReader,
        chunk::{
            ChunkBaseHeader, ChunkExtendedTimestamp, ChunkMessageHeader, ParseChunkError,
            VirtualMessageHeader,
        },
    },
};

const DEFAULT_CHUNK_SIZE: usize = 128;

pub(crate) struct RtmpMessageReader {
    reader: BufferedReader,
    context: HashMap<u32, ChunkStreamContext>,
    chunk_size: usize,
}

impl RtmpMessageReader {
    pub fn new(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        Self {
            reader: BufferedReader::new(socket, should_close),
            context: HashMap::new(),
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }

    pub fn next(&mut self) -> Option<RawMessage> {
        loop {
            match self.try_read_msg(self.reader.data()) {
                Ok(Some(msg)) => return Some(msg),
                Ok(None) => {}
                Err(ParseChunkError::NotEnoughData) => todo!(),
                Err(ParseChunkError::RtmpError(err)) => match err {
                    RtmpError::UnexpectedEof => return None,
                    RtmpError::Io(e) if e.kind() == ErrorKind::ConnectionReset => {
                        return None;
                    }
                    RtmpError::Io(e) if e.kind() == ErrorKind::BrokenPipe => return None,
                    err => {
                        warn!(%err)
                    }
                },
            }
        }
    }

    fn try_read_msg(&self, buffer: &VecDeque<u8>) -> Result<Option<RawMessage>, ParseChunkError> {
        let (base_header, offset) = ChunkBaseHeader::try_read(&buffer)?;
        let (msg_header, offset) = ChunkMessageHeader::try_read(&base_header, &buffer, offset)?;

        let context = self
            .context
            .entry(base_header.cs_id)
            .or_insert_with(Default::default);

        let msg_header = VirtualMessageHeader::from_msg(context.header, msg_header)?;

        let (extended_timestamp, offset) = match msg_header.timestamp == 0xFFFFFF {
            true => {
                let (ts, offset) = ChunkExtendedTimestamp::try_read(buffer, offset)?;
                (Some(ts), offset)
            }
            false => (None, offset),
        };

        let msg_len = msg_header.msg_len as usize;
        let payload_len = usize::min(
            self.chunk_size,
            msg_len.saturating_sub(context.payload_acc.len()),
        );
        if buffer.len() < offset + payload_len {
            return Err(ParseChunkError::NotEnoughData);
        }
        let payload = Bytes::from_iter(buffer.iter().skip(offset).take(payload_len).copied());

        // at this point whole chunk is in the buffer
        let msg = context.process_chunk(msg_header, extended_timestamp, payload)?;
        self.reader.drain(offset + payload_len);
        Ok(msg)
    }

    //fn try_complete_message(&mut self, chunk: &Chunk) -> Result<Option<RawMessage>, RtmpError> {
    //    let cs_id = chunk.cs_id;

    //    let acc = self
    //        .accumulators
    //        .entry(cs_id)
    //        .or_insert_with(|| PayloadAccumulator::new());

    //    match chunk.msg_header {
    //        MessageHeader::Full {
    //            timestamp,
    //            msg_len,
    //            msg_type_id,
    //            msg_stream_id,
    //        } => todo!(),
    //        MessageHeader::NoMessageStreamId {
    //            timestamp_delta,
    //            msg_len,
    //            msg_type_id,
    //        } => todo!(),
    //        MessageHeader::TimestampOnly { timestamp_delta } => todo!(),
    //        MessageHeader::NoHeader => todo!(),
    //    };

    //    let acc = self.accumulators.get_mut(&cs_id).unwrap();
    //    acc.append(chunk.payload);
    //    if let Some(message_payload) = acc.try_pop()? {
    //        let msg_type = MessageType::try_from_raw(chunk.header.msg_type_id)?;
    //        Ok(Some(RawMessage {
    //            timestamp: chunk.timestamp,
    //            msg_type,
    //            stream_id: chunk.msg_stream_id,
    //            payload: acc.buffer.freeze(),
    //        }))
    //    } else {
    //        Ok(None)
    //    }
    //}
}

//impl Iterator for RtmpMessageReader {
//    type Item = Result<RtmpMessage, RtmpError>;
//
//    fn next(&mut self) -> Option<Self::Item> {
//        loop {
//            let chunk = match self.chunk_reader.read_chunk(&self.accumulators) {
//                Ok(chunk) => chunk,
//                Err(RtmpError::UnexpectedEof) => return None,
//                Err(RtmpError::Io(e)) if e.kind() == ErrorKind::ConnectionReset => {
//                    return None;
//                }
//                Err(RtmpError::Io(e)) if e.kind() == ErrorKind::BrokenPipe => return None,
//                Err(e) => return Some(Err(e)),
//            };
//            if let Some(msg) = self.try_complete_message(chunk) {
//                return match RtmpMessage::from_raw(msg) {
//                    Ok(msg) => Some(Ok(msg)),
//                    Err(err) => Some(Err(err.into())),
//                };
//            }
//        }
//    }
//}

#[derive(Debug, Default)]
struct ChunkStreamContext {
    header: VirtualMessageHeader,
    timestamp: u32,
    payload_acc: VecDeque<Bytes>,
}

impl ChunkStreamContext {
    fn process_chunk(
        &mut self,
        msg_header: VirtualMessageHeader,
        extended_timestamp: Option<ChunkExtendedTimestamp>,
        payload: Bytes,
    ) -> Result<Option<RawMessage>, ParseChunkError> {
        self.header = msg_header;
        
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
