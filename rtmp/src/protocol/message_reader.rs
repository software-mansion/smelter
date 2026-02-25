use std::{
    collections::{HashMap, VecDeque},
    io::ErrorKind,
};

use bytes::{Bytes, BytesMut};

use crate::{
    error::RtmpError,
    message::RtmpMessage,
    protocol::{
        MessageType, RawMessage,
        chunk::{
            ChunkBaseHeader, ChunkExtendedTimestamp, ChunkHeaderTimestamp, ChunkMessageHeader,
            ParseChunkError, VirtualMessageHeader,
        },
        socket::BufferedReader,
    },
};

const DEFAULT_CHUNK_SIZE: usize = 128;

pub(crate) struct RtmpMessageReader {
    reader: BufferedReader,
    context: HashMap<u32, ChunkStreamContext>,
    chunk_size: usize,
}

impl RtmpMessageReader {
    pub fn new(reader: BufferedReader) -> Self {
        Self {
            reader,
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
        // Message header with all the information filled based on the context
        let msg_header = VirtualMessageHeader::from_msg(context.header, msg_header)?;

        // We need the context of the previous message to check if extended_timestamp
        // is present
        let (extended_timestamp, offset) = match msg_header.timestamp.has_extended() {
            true => {
                let (ts, offset) = ChunkExtendedTimestamp::try_read(buffer, offset)?;
                (Some(ts), offset)
            }
            false => (None, offset),
        };

        let (payload, offset) =
            Self::try_chunk_read_payload(buffer, offset, self.chunk_size, msg_header, context)?;

        // At this point whole chunk is in the buffer and we can remove that fragment from the
        // buffer.
        buffer.drain(..offset);

        let msg =
            context.process_chunk(base_header.cs_id, msg_header, extended_timestamp, payload)?;
        Ok(msg)
    }

    fn try_chunk_read_payload(
        buffer: &VecDeque<u8>,
        offset: usize,
        chunk_size: usize,
        msg_header: VirtualMessageHeader,
        context: &ChunkStreamContext,
    ) -> Result<(Bytes, usize), ParseChunkError> {
        // We need to check how long entire message is and how much is still missing
        let missing_payload_len =
            (msg_header.msg_len as usize).saturating_sub(context.buffered_payload_len());

        // Size of the payload can't be larger than chunk_size.
        let payload_len = usize::min(chunk_size, missing_payload_len);
        if buffer.len() < offset + payload_len {
            return Err(ParseChunkError::NotEnoughData);
        }

        let payload = Bytes::from_iter(buffer.iter().skip(offset).take(payload_len).copied());
        Ok((payload, offset + payload_len))
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
                Ok(None) => {}
                Err(ParseChunkError::NotEnoughData) => {
                    // read next chunk
                    let buf_len = self.reader.data().len();
                    if let Err(err) = self.reader.read_until_buffer_size(buf_len + 1) {
                        return Some(Err(err.into()));
                    }
                }
                Err(ParseChunkError::RtmpError(err)) => match err {
                    RtmpError::Io(e)
                        if [
                            ErrorKind::UnexpectedEof,
                            ErrorKind::ConnectionReset,
                            ErrorKind::BrokenPipe,
                        ]
                        .contains(&e.kind()) =>
                    {
                        return None;
                    }
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
        cs_id: u32,
        msg_header: VirtualMessageHeader,
        extended_timestamp: Option<ChunkExtendedTimestamp>,
        payload: Bytes,
    ) -> Result<Option<RawMessage>, RtmpError> {
        self.header = Some(msg_header);
        self.payload_acc.push_back(payload);
        let current_len = self.buffered_payload_len();

        if current_len < msg_header.msg_len as usize {
            return Ok(None);
        } else if current_len > msg_header.msg_len as usize {
            return Err(RtmpError::InternalError("Payload size too large"));
        }

        self.timestamp = match msg_header.timestamp {
            ChunkHeaderTimestamp::Timestamp(0x00FFFFFF) => {
                let Some(ChunkExtendedTimestamp(ext_ts)) = extended_timestamp else {
                    return Err(RtmpError::InternalError("Missing extended timestamp"));
                };
                ext_ts
            }
            ChunkHeaderTimestamp::Delta(0x00FFFFFF) => {
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
            chunk_stream_id: cs_id,
            stream_id: msg_header.msg_stream_id,
            timestamp: self.timestamp,
            payload: payload.freeze(),
        }))
    }

    fn buffered_payload_len(&self) -> usize {
        self.payload_acc.iter().map(|p| p.len()).sum()
    }
}
