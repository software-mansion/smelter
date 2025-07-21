use std::time::Duration;

use crate::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum ChunkFromFfmpegError {
    #[error("No data")]
    NoData,
    #[error("No pts")]
    NoPts,
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
