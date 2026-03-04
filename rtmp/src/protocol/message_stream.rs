use std::{
    collections::{HashMap, VecDeque},
    io::ErrorKind,
};

use bytes::{Bytes, BytesMut};
use tracing::{debug, trace};

use crate::{
    RtmpMessageSerializeError,
    error::RtmpStreamError,
    message::RtmpMessage,
    protocol::{
        RawMessage,
        byte_stream::RtmpByteStream,
        chunk::{
            ChunkBaseHeader, ChunkExtendedTimestamp, ChunkHeaderTimestamp, ChunkMessageHeader,
            ParseChunkError, VirtualMessageHeader,
        },
    },
};

const DEFAULT_CHUNK_SIZE: usize = 128;

pub(crate) struct RtmpMessageStream {
    stream: RtmpByteStream,
    reader: RtmpMessageReader,
    writer: RtmpMessageWriter,
}

impl RtmpMessageStream {
    pub fn new(socket: RtmpByteStream) -> Self {
        Self {
            stream: socket,
            reader: RtmpMessageReader::new(),
            writer: RtmpMessageWriter::new(),
        }
    }

    pub fn bytes_read(&self) -> u64 {
        self.stream.bytes_read()
    }

    pub fn set_reader_chunk_size(&mut self, size: usize) {
        self.reader.chunk_size = size;
    }

    pub fn set_writer_chunk_size(&mut self, size: usize) {
        self.writer.chunk_size = size;
    }

    pub fn read_msg(&mut self) -> Result<RtmpMessage, RtmpStreamError> {
        self.reader.read_msg(&mut self.stream)
    }

    pub fn try_read_msg(&mut self) -> Result<Option<RtmpMessage>, RtmpStreamError> {
        self.reader.try_read_msg(&mut self.stream)
    }

    pub fn write_msg(&mut self, msg: RtmpMessage) -> Result<(), RtmpStreamError> {
        self.writer.write_msg(&mut self.stream, msg)
    }
}

// ---- Reader ----

struct RtmpMessageReader {
    context: HashMap<u32, ReaderChunkStreamContext>,
    chunk_size: usize,
}

impl RtmpMessageReader {
    fn new() -> Self {
        Self {
            context: HashMap::new(),
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    fn read_msg(&mut self, socket: &mut RtmpByteStream) -> Result<RtmpMessage, RtmpStreamError> {
        self.next_chunk(socket, true)
    }

    fn try_read_msg(
        &mut self,
        socket: &mut RtmpByteStream,
    ) -> Result<Option<RtmpMessage>, RtmpStreamError> {
        match self.next_chunk(socket, false) {
            Ok(msg) => Ok(Some(msg)),
            Err(RtmpStreamError::TcpError(err)) => match err.kind() {
                ErrorKind::WouldBlock | ErrorKind::TimedOut => Ok(None),
                _ => Err(RtmpStreamError::TcpError(err)),
            },
            Err(err) => Err(err),
        }
    }

    fn next_chunk(
        &mut self,
        stream: &mut RtmpByteStream,
        force: bool,
    ) -> Result<RtmpMessage, RtmpStreamError> {
        loop {
            match self.try_parse_msg(stream.get_read_buffer_mut()) {
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
                        stream.read()?;
                    } else {
                        stream.try_read()?
                    }
                }
                Err(ParseChunkError::MalformedStream(err)) => {
                    return Err(RtmpStreamError::ReceivedMalformedStream(err));
                }
            }
        }
    }

    fn try_parse_msg(
        &mut self,
        buffer: &mut VecDeque<u8>,
    ) -> Result<Option<RawMessage>, ParseChunkError> {
        let (base_header, offset) = ChunkBaseHeader::try_read(buffer)?;
        let (msg_header, offset) = ChunkMessageHeader::try_read(&base_header, buffer, offset)?;

        let context = self.context.entry(base_header.cs_id).or_default();
        let msg_header = VirtualMessageHeader::from_msg(context.header, msg_header)
            .map_err(ParseChunkError::MalformedStream)?;

        let (ex_ts, offset) = match msg_header.timestamp.has_extended() {
            true => {
                let (ts, offset) = ChunkExtendedTimestamp::try_read(buffer, offset)?;
                (Some(ts), offset)
            }
            false => (None, offset),
        };

        let (payload, offset) =
            Self::try_chunk_read_payload(buffer, offset, self.chunk_size, msg_header, context)?;

        buffer.drain(..offset);

        let msg = context.process_chunk(base_header.cs_id, msg_header, ex_ts, payload)?;
        Ok(msg)
    }

    fn try_chunk_read_payload(
        buffer: &VecDeque<u8>,
        offset: usize,
        chunk_size: usize,
        msg_header: VirtualMessageHeader,
        context: &ReaderChunkStreamContext,
    ) -> Result<(Bytes, usize), ParseChunkError> {
        let missing_payload_len =
            (msg_header.msg_len as usize).saturating_sub(context.buffered_payload_len());
        let payload_len = usize::min(chunk_size, missing_payload_len);
        if buffer.len() < offset + payload_len {
            return Err(ParseChunkError::NotEnoughData);
        }
        let payload = Bytes::from_iter(buffer.iter().skip(offset).take(payload_len).copied());
        Ok((payload, offset + payload_len))
    }
}

#[derive(Debug, Default)]
struct ReaderChunkStreamContext {
    header: Option<VirtualMessageHeader>,
    timestamp: u32,
    payload_acc: VecDeque<Bytes>,
}

impl ReaderChunkStreamContext {
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

// ---- Writer ----

struct RtmpMessageWriter {
    context: HashMap<u32, WriterChunkStreamContext>,
    chunk_size: usize,
}

impl RtmpMessageWriter {
    fn new() -> Self {
        Self {
            context: HashMap::new(),
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    fn write_msg(
        &mut self,
        stream: &mut RtmpByteStream,
        msg: RtmpMessage,
    ) -> Result<(), RtmpStreamError> {
        match &msg {
            RtmpMessage::Event { event, .. } if event.is_media_packet() => {
                trace!(?msg, "Sending RTMP message")
            }
            msg => debug!(?msg, "Sending RTMP message"),
        }

        let msg = msg.into_raw()?;
        let cs_id = msg.chunk_stream_id;

        let context = self.context.entry(cs_id).or_default();

        let msg_header = context.resolve_header_type(&msg);
        let extended_timestamp = context.resolve_extended_timestamps(&msg_header, msg.timestamp);
        context.update(msg_header, msg.timestamp)?;

        let mut msg_header = Some(msg_header);
        let mut payload = msg.payload;
        let chunk_size = self.chunk_size;
        while !payload.is_empty() {
            let chunk_payload = match payload.len() > chunk_size {
                true => payload.split_to(chunk_size),
                false => payload.split_to(payload.len()),
            };
            let msg_header = msg_header.take().unwrap_or(ChunkMessageHeader::NoHeader);
            Self::write_chunk(stream, cs_id, msg_header, extended_timestamp, chunk_payload)?;
        }
        stream.flush()?;
        Ok(())
    }

    fn write_chunk(
        stream: &mut RtmpByteStream,
        cs_id: u32,
        msg_header: ChunkMessageHeader,
        extended_timestamp: Option<u32>,
        payload: Bytes,
    ) -> Result<(), RtmpStreamError> {
        let base_header = ChunkBaseHeader {
            fmt: msg_header.chunk_type(),
            cs_id,
        };

        let base_header_data = base_header.serialize()?;
        let msg_header_data = msg_header.serialize();

        stream.write(&base_header_data)?;
        stream.write(&msg_header_data)?;
        if let Some(ex_ts) = extended_timestamp {
            stream.write(&ex_ts.to_be_bytes())?;
        }
        stream.write(&payload)?;

        Ok(())
    }
}

#[derive(Debug, Default)]
struct WriterChunkStreamContext(Option<(VirtualMessageHeader, u32)>);

impl WriterChunkStreamContext {
    fn resolve_header_type(&self, msg: &RawMessage) -> ChunkMessageHeader {
        let Some((prev, prev_ts)) = self.0 else {
            return ChunkMessageHeader::Full {
                timestamp: msg.timestamp,
                msg_len: msg.payload.len() as u32,
                msg_type_id: msg.msg_type,
                msg_stream_id: msg.stream_id,
            };
        };

        let msg_len_match = prev.msg_len == msg.payload.len() as u32;
        let msg_type_id_match = prev.msg_type_id == msg.msg_type;
        let msg_stream_id_match = prev.msg_stream_id == msg.stream_id;

        let timestamp_delta = msg.timestamp.saturating_sub(prev_ts);

        let msg_timestamp_match = (prev.timestamp.has_extended() && timestamp_delta >= 0x00FFFFFF)
            || (!prev.timestamp.has_extended() && prev.timestamp.value() == timestamp_delta);

        if !msg_stream_id_match {
            return ChunkMessageHeader::Full {
                timestamp: match msg.timestamp >= 0x00FFFFFF {
                    true => 0x00FFFFFF,
                    false => msg.timestamp,
                },
                msg_len: msg.payload.len() as u32,
                msg_type_id: msg.msg_type,
                msg_stream_id: msg.stream_id,
            };
        }

        if msg_stream_id_match && (!msg_len_match || !msg_type_id_match) {
            return ChunkMessageHeader::NoMessageStreamId {
                timestamp_delta: match timestamp_delta >= 0x00FFFFFF {
                    true => 0x00FFFFFF,
                    false => timestamp_delta,
                },
                msg_len: msg.payload.len() as u32,
                msg_type_id: msg.msg_type,
            };
        }

        if msg_stream_id_match && msg_type_id_match && msg_len_match && !msg_timestamp_match {
            return ChunkMessageHeader::TimestampOnly {
                timestamp_delta: match timestamp_delta >= 0x00FFFFFF {
                    true => 0x00FFFFFF,
                    false => timestamp_delta,
                },
            };
        }

        if msg_stream_id_match && msg_type_id_match && msg_len_match && msg_timestamp_match {
            return ChunkMessageHeader::NoHeader;
        }

        unreachable!()
    }

    fn resolve_extended_timestamps(&self, msg: &ChunkMessageHeader, timestamp: u32) -> Option<u32> {
        let Some((_, prev_ts)) = self.0 else {
            return match timestamp >= 0x00FFFFFF {
                true => Some(timestamp),
                false => None,
            };
        };
        let delta = timestamp.saturating_sub(prev_ts);
        match msg {
            ChunkMessageHeader::Full { .. } if timestamp >= 0x00FFFFFF => Some(timestamp),
            ChunkMessageHeader::NoMessageStreamId { .. } if delta >= 0x00FFFFFF => Some(delta),
            ChunkMessageHeader::TimestampOnly { .. } if delta >= 0x00FFFFFF => Some(delta),
            ChunkMessageHeader::NoHeader if delta >= 0x00FFFFFF => Some(delta),
            _ => None,
        }
    }

    fn update(&mut self, msg: ChunkMessageHeader, timestamp: u32) -> Result<(), RtmpStreamError> {
        let prev = self.0.map(|prev| prev.0);
        let header = VirtualMessageHeader::from_msg(prev, msg)
            .map_err(RtmpMessageSerializeError::InternalError)?;
        self.0 = Some((header, timestamp));
        Ok(())
    }
}
