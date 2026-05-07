mod audio;
mod ex_audio;
mod ex_video;
mod mod_ex_audio;
mod mod_ex_video;
mod video;

pub use audio::*;
use bytes::Bytes;
pub use ex_audio::*;
pub use ex_video::*;
pub use video::*;

use crate::{FlvAudioTagParseError, FlvVideoTagParseError, RtmpMessageSerializeError};

pub(super) const EX_HEADER_BIT: u8 = 0b10000000;
const EX_AUDIO_SOUND_FORMAT: u8 = 9;

/// Top-level FLV audio data, supporting both legacy and Enhanced RTMP formats.
///
/// Legacy format: <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=75>
/// Enhanced RTMP: <https://veovera.org/docs/enhanced/enhanced-rtmp-v2.pdf>
#[derive(Debug, Clone)]
pub enum FlvAudioData {
    Legacy(AudioTag),
    Enhanced(ExAudioTag),
}

impl FlvAudioData {
    #[allow(dead_code)]
    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        match self {
            FlvAudioData::Legacy(tag) => tag.serialize(),
            FlvAudioData::Enhanced(tag) => tag.serialize(),
        }
    }

    /// Parses flv `AUDIODATA`. Checks SoundFormat in the first byte and
    /// dispatches to either legacy or Enhanced RTMP parsing.
    pub fn parse(data: Bytes) -> Result<Self, FlvAudioTagParseError> {
        if data.is_empty() {
            return Err(FlvAudioTagParseError::TooShort);
        }

        let sound_format = (data[0] & 0b11110000) >> 4;
        if sound_format == EX_AUDIO_SOUND_FORMAT {
            ExAudioTag::parse(data).map(FlvAudioData::Enhanced)
        } else {
            AudioTag::parse(data).map(FlvAudioData::Legacy)
        }
    }
}

/// Top-level FLV video data, supporting both legacy and Enhanced RTMP formats.
///
/// Legacy format: <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74>
/// Enhanced RTMP: <https://veovera.org/docs/enhanced/enhanced-rtmp-v2.pdf>
#[derive(Debug, Clone)]
pub enum FlvVideoData {
    Legacy(VideoTag),
    Enhanced(ExVideoTag),
}

impl FlvVideoData {
    /// Parses flv `VIDEODATA`. Checks the IsExHeader bit in the first byte
    /// and dispatches to either legacy or Enhanced RTMP parsing.
    pub fn parse(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        if data.is_empty() {
            return Err(FlvVideoTagParseError::TooShort);
        }

        if data[0] & EX_HEADER_BIT != 0 {
            ExVideoTag::parse(data).map(FlvVideoData::Enhanced)
        } else {
            VideoTag::parse(data).map(FlvVideoData::Legacy)
        }
    }

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        match self {
            FlvVideoData::Legacy(tag) => tag.serialize(),
            FlvVideoData::Enhanced(tag) => tag.serialize(),
        }
    }
}
