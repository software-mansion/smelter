mod input;

pub(super) mod connection;
pub(super) mod state;

pub use input::RtmpServerInput;
use rtmp::{RtmpAudioCodec, RtmpVideoCodec};

use crate::prelude::*;

impl From<RtmpVideoCodec> for VideoCodec {
    fn from(value: RtmpVideoCodec) -> Self {
        match value {
            RtmpVideoCodec::H264 => VideoCodec::H264,
            RtmpVideoCodec::Vp8 => VideoCodec::Vp8,
            RtmpVideoCodec::Vp9 => VideoCodec::Vp9,
        }
    }
}

impl From<RtmpAudioCodec> for AudioCodec {
    fn from(value: RtmpAudioCodec) -> Self {
        match value {
            RtmpAudioCodec::Aac => AudioCodec::Aac,
            RtmpAudioCodec::Opus => AudioCodec::Opus,
        }
    }
}
