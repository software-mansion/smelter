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

use crate::{AudioChannels, RtmpSerializationMode};

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
pub(crate) enum RtmpMessageIncoming {
    /// Protocol control messages
    /// - message stream id 0
    /// - chunk stream id 2
    SetChunkSize {
        chunk_size: u32,
    },
    // TODO: AbortMessage,
    Acknowledgement {
        // TODO: use for send throttling
        #[allow(unused)]
        bytes_received: u32,
    },
    WindowAckSize {
        window_size: u32,
    },
    SetPeerBandwidth {
        bandwidth: u32,
        // TODO: adjust behavior based on Hard/Soft/Dynamic mode
        #[allow(unused)]
        limit_type: u8,
    },

    UserControl(UserControlMessage),
    CommandMessage {
        msg: CommandMessage,
        stream_id: u32,
    },

    // Video, Audio, and DataMessage don't carry stream_id because we only
    // support one media stream per incoming connection, so stream_id is
    // always the same and redundant.
    Video {
        video: VideoMessage,
    },

    Audio {
        audio: AudioMessage,
    },

    DataMessage {
        data: DataMessage,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum RtmpMessageOutgoing {
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
        serialization_mode: RtmpSerializationMode,
    },

    Audio {
        audio: AudioMessage,
        stream_id: u32,
        channels: AudioChannels,
        serialization_mode: RtmpSerializationMode,
    },

    DataMessage {
        data: DataMessage,
        stream_id: u32,
    },
}

impl RtmpMessageIncoming {
    pub fn is_media_packet(&self) -> bool {
        match self {
            Self::Video { video, .. } => video.is_media_packet(),
            Self::Audio { audio, .. } => audio.is_media_packet(),
            _ => false,
        }
    }
}

impl RtmpMessageOutgoing {
    pub fn is_media_packet(&self) -> bool {
        match self {
            Self::Video { video, .. } => video.is_media_packet(),
            Self::Audio { audio, .. } => audio.is_media_packet(),
            _ => false,
        }
    }
}
