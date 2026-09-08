use std::{path::Path, sync::Arc, time::Duration};

use crate::Timestamp;
use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct Mp4InputOptions {
    pub source: Mp4InputSource,
    pub should_loop: bool,
    pub video_decoders: Mp4InputVideoDecoders,
    pub seek: Option<Duration>,
    pub offset: Option<Duration>,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mp4OutputOptions {
    pub output_path: Arc<Path>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
    pub raw_options: Vec<(Arc<str>, Arc<str>)>,
    /// If set, the output is created immediately, but starts producing data only after
    /// the queue reaches this timestamp (relative to the queue start). It doubles as the
    /// timestamp offset of the produced file, so PTS 0 in the file is exactly this
    /// moment rather than whenever the first chunk happened to be encoded.
    pub start_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mp4InputSource {
    Url(Arc<str>),
    File(Arc<Path>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mp4InputVideoDecoders {
    pub h264: Option<VideoDecoderOptions>,
}

#[derive(Debug, thiserror::Error)]
pub enum Mp4InputError {
    #[error("Error while doing file operations.")]
    IoError(#[from] std::io::Error),

    #[error("Error while downloading the MP4 file.")]
    HttpError(#[from] reqwest::Error),

    #[error("Mp4 reader error.")]
    Mp4ReaderError(#[from] mp4::Error),

    #[error("No suitable track in the mp4 file")]
    NoTrack,

    #[error("Unknown error: {0}")]
    Unknown(&'static str),
}
