use crate::{RtmpEvent, amf0::Amf0Value};

mod event;
mod parse;
mod serialize;

#[derive(Debug)]
pub(crate) enum RtmpMessage {
    WindowAckSize {
        window_size: u32,
    },
    SetPeerBandwidth {
        bandwidth: u32,
        limit_type: u8,
    },
    StreamBegin {
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
    SetChunkSize {
        chunk_size: u32,
    },
    Event {
        event: RtmpEvent,
        stream_id: u32,
    },
}
