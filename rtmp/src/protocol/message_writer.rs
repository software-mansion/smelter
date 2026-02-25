use std::{collections::HashMap, io::Write};

use bytes::Bytes;
use tracing::trace;

use crate::{
    error::RtmpError,
    message::RtmpMessage,
    protocol::{
        RawMessage,
        chunk::{ChunkBaseHeader, ChunkMessageHeader, VirtualMessageHeader},
        socket::BufferedWriter,
    },
};

pub struct RtmpMessageWriter {
    stream: BufferedWriter,
    chunk_size: usize,
    context: HashMap<u32, ChunkStreamContext>,
}

impl RtmpMessageWriter {
    pub fn new(stream: BufferedWriter) -> Self {
        Self {
            stream,
            chunk_size: 128,
            context: HashMap::new(),
        }
    }

    #[allow(unused)]
    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }

    pub fn write(&mut self, msg: RtmpMessage) -> Result<(), RtmpError> {
        trace!(?msg, "Sending RTMP message");
        let msg = msg.into_raw()?;
        let cs_id = msg.chunk_stream_id;

        let context = self.context.entry(cs_id).or_default();

        // First header before division to chunks
        let msg_header = context.resolve_header_type(&msg);
        let extended_timestamp = context.resolve_extended_timestamps(&msg_header, msg.timestamp);
        context.update(msg_header, msg.timestamp)?;

        let mut msg_header = Some(msg_header);
        let mut payload = msg.payload;
        while !payload.is_empty() {
            let payload = match payload.len() > self.chunk_size {
                true => payload.split_to(self.chunk_size),
                false => payload.split_to(payload.len()),
            };
            let msg_header = msg_header.take().unwrap_or(ChunkMessageHeader::NoHeader);
            self.write_chunk(cs_id, msg_header, extended_timestamp, payload)?;
        }
        self.stream.flush()?;
        Ok(())
    }

    pub fn write_chunk(
        &mut self,
        cs_id: u32,
        msg_header: ChunkMessageHeader,
        extended_timestamp: Option<u32>,
        payload: Bytes,
    ) -> Result<(), RtmpError> {
        let base_header = ChunkBaseHeader {
            fmt: msg_header.chunk_type(),
            cs_id,
        };

        self.stream.write_all(&base_header.serialize()?)?;
        self.stream.write_all(&msg_header.serialize())?;
        if let Some(ex_ts) = extended_timestamp {
            self.stream.write_all(&ex_ts.to_be_bytes())?;
        }
        self.stream.write_all(&payload)?;

        Ok(())
    }
}

#[derive(Debug, Default)]
struct ChunkStreamContext(Option<(VirtualMessageHeader, u32)>);

impl ChunkStreamContext {
    fn resolve_header_type(&self, msg: &RawMessage) -> ChunkMessageHeader {
        let Some((prev, prev_ts)) = self.0 else {
            return ChunkMessageHeader::Full {
                timestamp: msg.timestamp,
                msg_len: msg.payload.len() as u32,
                msg_type_id: msg.msg_type.into_raw(),
                msg_stream_id: msg.stream_id,
            };
        };

        let msg_len_match = prev.msg_len == msg.payload.len() as u32;
        let msg_type_id_match = prev.msg_type_id == msg.msg_type.into_raw();
        let msg_stream_id_match = prev.msg_stream_id == msg.stream_id;

        let timestamp_delta = msg.timestamp.saturating_sub(prev_ts);

        // true if (one of the conditions):
        // - both prev and current message requires extended timestamps
        // - previous message timestamp or timestamp delta is the same as current msg delta
        let msg_timestamp_match = (prev.timestamp.has_extended() && timestamp_delta >= 0x00FFFFFF)
            || (!prev.timestamp.has_extended() && prev.timestamp.value() == timestamp_delta);

        if !msg_stream_id_match {
            return ChunkMessageHeader::Full {
                timestamp: match msg.timestamp >= 0x00FFFFFF {
                    true => 0x00FFFFFF,
                    false => msg.timestamp,
                },
                msg_len: msg.payload.len() as u32,
                msg_type_id: msg.msg_type.into_raw(),
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
                msg_type_id: msg.msg_type.into_raw(),
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

        // This is Type-3 for entire message, division to chunks will happen later
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

    fn update(&mut self, msg: ChunkMessageHeader, timestamp: u32) -> Result<(), RtmpError> {
        let prev = self.0.map(|prev| prev.0);
        let header = VirtualMessageHeader::from_msg(prev, msg)?;
        self.0 = Some((header, timestamp));
        Ok(())
    }
}
