use std::path::PathBuf;

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct Mp4InputOptions {
    pub source: Mp4InputSource,
    pub should_loop: bool,
    pub video_decoder: VideoDecoderOptions,
}

#[derive(Debug, Clone)]
pub struct Mp4OutputOptions {
    pub output_path: PathBuf,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub enum Mp4InputSource {
    Url(String),
    File(PathBuf),
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
