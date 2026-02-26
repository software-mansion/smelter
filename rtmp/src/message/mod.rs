use crate::{RtmpEvent, amf0::Amf0Value};

mod event;
mod parse;
mod serialize;
mod user_control;

pub(crate) use user_control::UserControlMessage;

// Low-level protocol control messages and commands
const RESERVED_CHUNK_STREAM_ID: u32 = 2;
// Main chunk stream for everything that is not actual media
const MAIN_CHUNK_STREAM_ID: u32 = 3;
const VIDEO_CHUNK_STREAM_ID: u32 = 6;
const AUDIO_CHUNK_STREAM_ID: u32 = 4;

#[derive(Debug)]
pub(crate) enum RtmpMessage {
    WindowAckSize {
        window_size: u32,
    },
    SetPeerBandwidth {
        bandwidth: u32,
        limit_type: u8,
    },
    UserControl(UserControlMessage),

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
