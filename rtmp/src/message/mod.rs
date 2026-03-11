use crate::RtmpEvent;

mod command;
mod event;
mod parse;
mod serialize;
mod user_control;

pub(crate) use command::{
    CommandMessage, CommandMessageConnectSuccess, CommandMessageCreateStreamSuccess,
    CommandMessageOk, CommandMessageResultExt,
};
pub(crate) use user_control::UserControlMessage;

//
// Chunk stream ids
//

/// Low-level protocol control messages and commands
const RESERVED_CHUNK_STREAM_ID: u32 = 2;
/// Main chunk stream for everything that is not actual media
const MAIN_CHUNK_STREAM_ID: u32 = 3;
const VIDEO_CHUNK_STREAM_ID: u32 = 6;
const AUDIO_CHUNK_STREAM_ID: u32 = 4;

//
// Message stream ids
//

pub(crate) const CONTROL_MESSAGE_STREAM_ID: u32 = 0;

#[derive(Debug)]
pub(crate) enum RtmpMessage {
    /// Protocol control messages
    /// - message stream id 0
    /// - chunk stream id 2
    SetChunkSize {
        chunk_size: u32,
    },
    // TODO: AbortMessage,
    Acknowledgement {
        bytes_received: u32,
    },
    WindowAckSize {
        window_size: u32,
    },
    SetPeerBandwidth {
        bandwidth: u32,
        limit_type: u8,
    },

    UserControl(UserControlMessage),
    CommandMessage {
        msg: CommandMessage,
        stream_id: u32,
    },

    Event {
        event: RtmpEvent,
        stream_id: u32,
    },
}
