use std::{fmt, time::Duration};

use bytes::Bytes;
use compositor_render::Frame;
use crossbeam_channel::{Receiver, Sender};

use crate::{
    audio_mixer::{AudioSamples, InputSamples, OutputSamples},
    queue::PipelineEvent,
};

/// A struct representing a chunk of encoded data.
///
/// Many codecs specify that encoded data is split into chunks.
/// For example, H264 splits the data into NAL units and AV1 splits the data into OBU frames.
pub struct EncodedChunk {
    pub data: Bytes,
    pub pts: Duration,
    pub dts: Option<Duration>,
    pub is_keyframe: IsKeyframe,
    pub kind: EncodedChunkKind,
}

pub enum IsKeyframe {
    /// this is a keyframe
    Yes,
    /// this is not a keyframe
    No,
    /// it's unknown whether this frame is a keyframe or not
    Unknown,
    /// the codec this chunk is encoded in does not have keyframes at all
    NoKeyframes,
}

#[derive(Debug)]
pub enum EncoderOutputEvent {
    Data(EncodedChunk),
    AudioEOS,
    VideoEOS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedChunkKind {
    Video(VideoCodec),
    Audio(AudioCodec),
}

#[derive(Debug, thiserror::Error)]
pub enum ChunkFromFfmpegError {
    #[error("No data")]
    NoData,
    #[error("No pts")]
    NoPts,
}

/// Receiver sides of video/audio channels for data produced by
/// audio mixer and renderer
#[derive(Debug, Clone)]
pub struct RawDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<OutputSamples>>>,
}

#[derive(Debug)]
pub struct RawDataSender {
    pub video: Option<Sender<PipelineEvent<Frame>>>,
    pub audio: Option<Sender<PipelineEvent<InputSamples>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    Vp8,
    Vp9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Aac,
    Opus,
}

#[derive(Debug, thiserror::Error)]
pub enum CodecFromFfmpegError {
    #[error("Unsupported codec {0:?}")]
    UnsupportedCodec(ffmpeg_next::codec::Id),
}

impl TryFrom<ffmpeg_next::Codec> for VideoCodec {
    type Error = CodecFromFfmpegError;

    fn try_from(value: ffmpeg_next::Codec) -> Result<Self, Self::Error> {
        match value.id() {
            ffmpeg_next::codec::Id::H264 => Ok(Self::H264),
            v => Err(CodecFromFfmpegError::UnsupportedCodec(v)),
        }
    }
}

/// Raw samples produced by a decoder or received from external source.
/// They still need to be resampled before passing them to the queue.
#[derive(Debug)]
pub(crate) struct DecodedSamples {
    pub samples: AudioSamples,
    pub start_pts: Duration,
    pub sample_rate: u32,
}

impl fmt::Debug for EncodedChunk {
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
