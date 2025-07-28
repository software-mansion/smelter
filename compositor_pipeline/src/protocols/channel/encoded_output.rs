use core::fmt;
use std::time::Duration;

use crate::{
    codecs::{AudioEncoderOptions, VideoEncoderOptions},
    MediaKind,
};

/// Options to configure output that sends encoded audio and video chunks via single channel
#[derive(Debug, Clone)]
pub struct EncodedDataOutputOptions {
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug)]
pub enum EncodedOutputEvent {
    Data(EncodedOutputChunk),
    AudioEOS,
    VideoEOS,
}

/// A struct representing a chunk of encoded data.
///
/// Many codecs specify that encoded data is split into chunks.
/// For example, H264 splits the data into NAL units and AV1 splits the data into OBU frames.
pub struct EncodedOutputChunk {
    pub data: bytes::Bytes,
    pub pts: Duration,
    pub dts: Option<Duration>,
    pub is_keyframe: bool,
    pub kind: MediaKind,
}

impl fmt::Debug for EncodedOutputChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.data.len();
        let first_bytes = &self.data[0..usize::min(10, len)];
        f.debug_struct("EncodedChunk")
            .field("data", &format!("len={len}, {first_bytes:?}"))
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("kind", &self.kind)
            .finish()
    }
}
