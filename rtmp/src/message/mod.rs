use bytes::Bytes;

use crate::{RtmpEvent, amf0::Amf0Value};

mod event;
mod parse;
mod serialize;

#[derive(Debug)]
pub(crate) enum RtmpMessage {
    SetChunkSize {
        chunk_size: u32,
    },
    AbortMessage {
        chunk_stream_id: u32,
    },
    Acknowledgement {
        sequence_number: u32,
    },
    UserControl {
        event: UserControlEvent,
    },
    WindowAckSize {
        window_size: u32,
    },
    SetPeerBandwidth {
        bandwidth: u32,
        limit_type: u8,
    },
    // Audio/Video/Metadata events
    Event {
        event: RtmpEvent,
        stream_id: u32,
    },
    // Explanation why it is a sequence of amf0 values and not amf3 values:
    // https://zenomt.github.io/rtmp-errata-addenda/rtmp-errata-addenda.html#name-object-encoding-3-2
    CommandMessageAmf3 {
        values: Vec<Amf0Value>,
        stream_id: u32,
    },
    CommandMessageAmf0 {
        values: Vec<Amf0Value>,
        stream_id: u32,
    },
    SharedObjectAmf0 {
        name: String,
        version: u32,
        persistent: bool,
        events: Vec<SharedObjectEvent>,
    },
    SharedObjectAmf3 {
        name: String,
        version: u32,
        persistent: bool,
        events: Vec<SharedObjectEvent>,
    },
    AggregateMessage {
        stream_id: u32,
        messages: Vec<RtmpMessage>,
    },
}

// https://rtmp.veriskope.com/docs/spec/#717user-control-message-events
#[derive(Debug)]
pub(crate) enum UserControlEvent {
    StreamBegin {
        stream_id: u32,
    },
    StreamEof {
        stream_id: u32,
    },
    StreamDry {
        stream_id: u32,
    },
    SetBufferLength {
        stream_id: u32,
        buffer_length_ms: u32,
    },
    StreamIsRecorded {
        stream_id: u32,
    },
    PingRequest {
        timestamp: u32,
    },
    PingResponse {
        timestamp: u32,
    },
    // fallback for unrecognised event types.
    Unknown {
        event_type: u16,
        data: Bytes,
    },
}

/// A single event entry inside a Shared Object message.
/// `data` is raw bytes whose encoding (AMF0 or AMF3) depends on the parent message type.
#[derive(Debug, Clone)]
pub(crate) struct SharedObjectEvent {
    /// Event type code (1 = Use, 2 = Release, 3 = RequestChange, 4 = Change,
    /// 5 = Success, 6 = SendMessage, 7 = Status, 8 = ClearData,
    /// 9 = DeleteData, 10 = RequestRemove, 11 = UseSuccess).
    pub event_type: u8,
    pub data: Bytes,
}
