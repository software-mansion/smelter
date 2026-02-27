use std::{
    collections::{HashMap, VecDeque},
    io::ErrorKind,
};

use bytes::{Bytes, BytesMut};
use tracing::{debug, trace};

use crate::{
    error::RtmpStreamError,
    message::RtmpMessage,
    protocol::{
        RawMessage,
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

    pub fn bytes_read(&self) -> u64 {
        self.reader.bytes_read()
    }

    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }

    pub fn next(&mut self) -> Result<RtmpMessage, RtmpStreamError> {
        self.next_chunk(true)
    }

    pub fn try_next(&mut self) -> Result<Option<RtmpMessage>, RtmpStreamError> {
        match self.next_chunk(false) {
            Ok(msg) => Ok(Some(msg)),
            Err(RtmpStreamError::TcpError(err)) if err.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub fn next_chunk(&mut self, force: bool) -> Result<RtmpMessage, RtmpStreamError> {
        loop {
            match self.try_read_msg() {
                Ok(Some(msg)) => {
                    let msg = RtmpMessage::from_raw(msg)?;
                    match &msg {
                        RtmpMessage::Event { event, .. } if event.is_media_packet() => {
                            trace!(?msg, "Received RTMP message")
                        }
                        msg => debug!(?msg, "Received RTMP message"),
                    }
                    return Ok(msg);
                }
                Ok(None) => {}
                Err(ParseChunkError::NotEnoughData) => {
                    if force {
                        // read next chunk
                        let buf_len = self.reader.data().len();
                        self.reader.read_until_buffer_size(buf_len + 1)?;
                    } else {
                        self.reader.try_read()?
                    }
                }
                Err(ParseChunkError::MalformedStream(err)) => {
                    return Err(RtmpStreamError::ReceivedMalformedStream(err));
                }
            }
        }
    }

    fn try_read_msg(&mut self) -> Result<Option<RawMessage>, ParseChunkError> {
        let buffer = self.reader.data_mut();
        let (base_header, offset) = ChunkBaseHeader::try_read(buffer)?;
        let (msg_header, offset) = ChunkMessageHeader::try_read(&base_header, buffer, offset)?;

        let context = self.context.entry(base_header.cs_id).or_default();
        // Message header with all the information filled based on the context
        let msg_header = VirtualMessageHeader::from_msg(context.header, msg_header)
            .map_err(ParseChunkError::MalformedStream)?;

        // We need the context of the previous message to check if extended_timestamp
        // is present
        let (ex_ts, offset) = match msg_header.timestamp.has_extended() {
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

        let msg = context.process_chunk(base_header.cs_id, msg_header, ex_ts, payload)?;
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
    ) -> Result<Option<RawMessage>, ParseChunkError> {
        self.header = Some(msg_header);
        self.payload_acc.push_back(payload);
        let current_len = self.buffered_payload_len();

        if current_len < msg_header.msg_len as usize {
            return Ok(None);
        } else if current_len > msg_header.msg_len as usize {
            return Err(ParseChunkError::MalformedStream(
                "Payload size too large".into(),
            ));
        }

        self.timestamp = match msg_header.timestamp {
            ChunkHeaderTimestamp::Timestamp(0x00FFFFFF) => {
                let Some(ChunkExtendedTimestamp(ext_ts)) = extended_timestamp else {
                    return Err(ParseChunkError::MalformedStream(
                        "Missing extended timestamp".into(),
                    ));
                };
                ext_ts
            }
            ChunkHeaderTimestamp::Delta(0x00FFFFFF) => {
                let Some(ChunkExtendedTimestamp(ext_ts)) = extended_timestamp else {
                    return Err(ParseChunkError::MalformedStream(
                        "Missing extended timestamp".into(),
                    ));
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
            msg_type: msg_header.msg_type_id,
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
