use std::{
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
        buffered_stream_reader::BufferedReader,
        chunk::{
            ChunkBaseHeader, ChunkExtendedTimestamp, ChunkHeaderTimestamp, ChunkMessageHeader,
            ParseChunkError, VirtualMessageHeader,
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

    fn try_read_msg(&mut self) -> Result<Option<RawMessage>, ParseChunkError> {
        let buffer = self.reader.data_mut();
        let (base_header, offset) = ChunkBaseHeader::try_read(buffer)?;
        let (msg_header, offset) = ChunkMessageHeader::try_read(&base_header, buffer, offset)?;

        let context = self.context.entry(base_header.cs_id).or_default();

        let msg_header = VirtualMessageHeader::from_msg(context.header, msg_header)?;

        let (extended_timestamp, offset) = match msg_header.timestamp.has_extended() {
            true => {
                let (ts, offset) = ChunkExtendedTimestamp::try_read(buffer, offset)?;
                (Some(ts), offset)
            }
            false => (None, offset),
        };

        // Current chunk size can be calculated based on max_chunk size
        // and the message fragment we have already read
        let msg_len = msg_header.msg_len as usize;
        let payload_len = usize::min(
            self.chunk_size,
            msg_len.saturating_sub(context.payload_acc.len()),
        );
        if buffer.len() < offset + payload_len {
            return Err(ParseChunkError::NotEnoughData);
        }
        let payload = Bytes::from_iter(buffer.iter().skip(offset).take(payload_len).copied());

        // At this point whole chunk is in the buffer and we can remove that fragment from the
        // buffer.
        buffer.drain(..offset + payload_len);

        let msg = context.process_chunk(msg_header, extended_timestamp, payload)?;
        Ok(msg)
    }
}

impl Iterator for RtmpMessageReader {
    type Item = Result<RtmpMessage, RtmpError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.try_read_msg() {
                Ok(Some(msg)) => match RtmpMessage::from_raw(msg) {
                    Ok(msg) => return Some(Ok(msg)),
                    Err(err) => return Some(Err(err.into())),
                },
                Ok(None) | Err(ParseChunkError::NotEnoughData) => {
                    // read next chunk
                    let buf_len = self.reader.data().len();
                    if let Err(err) = self.reader.read_until_buffer_size(buf_len + 1) {
                        return Some(Err(err));
                    }
                }
                Err(ParseChunkError::RtmpError(err)) => match err {
                    RtmpError::UnexpectedEof => return None,
                    RtmpError::Io(e) if e.kind() == ErrorKind::ConnectionReset => {
                        return None;
                    }
                    RtmpError::Io(e) if e.kind() == ErrorKind::BrokenPipe => return None,
                    err => return Some(Err(err)),
                },
            }
        }
    }
}

#[derive(Debug, Default)]
struct ChunkStreamContext {
    header: Option<VirtualMessageHeader>,
    // real timestamp that takes into account both extended timestamps
    // and calculates all the timestamp deltas to absolute values.
    timestamp: u32,
    payload_acc: VecDeque<Bytes>,
}

impl ChunkStreamContext {
    fn process_chunk(
        &mut self,
        msg_header: VirtualMessageHeader,
        extended_timestamp: Option<ChunkExtendedTimestamp>,
        payload: Bytes,
    ) -> Result<Option<RawMessage>, RtmpError> {
        self.header = Some(msg_header);
        self.payload_acc.push_back(payload);
        let current_len = self.payload_acc.iter().map(|p| p.len()).sum();

        if current_len < msg_header.msg_len as usize {
            return Ok(None);
        } else if current_len > msg_header.msg_len as usize {
            return Err(RtmpError::InternalError("Payload size too large"));
        }

        self.timestamp = match msg_header.timestamp {
            ChunkHeaderTimestamp::Timestamp(0xFFFFFF) => {
                let Some(ChunkExtendedTimestamp(ext_ts)) = extended_timestamp else {
                    return Err(RtmpError::InternalError("Missing extended timestamp"));
                };
                ext_ts
            }
            ChunkHeaderTimestamp::Delta(0xFFFFFF) => {
                let Some(ChunkExtendedTimestamp(ext_ts)) = extended_timestamp else {
                    return Err(RtmpError::InternalError("Missing extended timestamp"));
                };
                self.timestamp + ext_ts
            }
            ChunkHeaderTimestamp::Timestamp(ts) => ts,
            ChunkHeaderTimestamp::Delta(ts) => self.timestamp + ts,
        };

        let mut payload = BytesMut::with_capacity(current_len);
        while let Some(chunk) = self.payload_acc.pop_front() {
            payload.extend_from_slice(&chunk);
        }

        Ok(Some(RawMessage {
            msg_type: MessageType::try_from_raw(msg_header.msg_type_id)?,
            stream_id: msg_header.msg_stream_id,
            timestamp: self.timestamp,
            payload: payload.freeze(),
        }))
    }
}
