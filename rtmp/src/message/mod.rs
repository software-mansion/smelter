mod audio;
mod command;
mod data;
mod parse;
mod serialize;
mod user_control;
mod video;

pub(crate) use audio::AudioMessage;
pub(crate) use command::{
    CommandMessage, CommandMessageConnectSuccess, CommandMessageCreateStreamSuccess,
    CommandMessageOk, CommandMessageResultExt,
};
pub(crate) use data::DataMessage;
pub(crate) use user_control::UserControlMessage;
pub(crate) use video::VideoMessage;

//
// Chunk stream ids
//

/// Low-level protocol control messages and commands
const PROTOCOL_CHUNK_STREAM_ID: u32 = 2;
/// Main chunk stream for everything that is not actual media
/// e.g. command or data messages
const MAIN_CHUNK_STREAM_ID: u32 = 3;
const VIDEO_CHUNK_STREAM_ID: u32 = 6;
const AUDIO_CHUNK_STREAM_ID: u32 = 4;

//
// Message stream ids
//

pub(crate) const CONTROL_MESSAGE_STREAM_ID: u32 = 0;

#[derive(Debug, Clone)]
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

    Video {
        video: VideoMessage,
        stream_id: u32,
    },

    Audio {
        audio: AudioMessage,
        stream_id: u32,
    },

    DataMessage {
        data: DataMessage,
        stream_id: u32,
    },
}

impl RtmpMessage {
    pub fn is_media_packet(&self) -> bool {
        match self {
            Self::Video { video, .. } => video.is_media_packet(),
            Self::Audio { audio, .. } => audio.is_media_packet(),
            _ => false,
        }
    }
}
