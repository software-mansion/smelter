use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    InputBufferOptions,
    codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions},
};

#[derive(Debug, Clone)]
pub struct Mp4InputOptions {
    pub source: Mp4InputSource,
    pub should_loop: bool,
    pub video_decoders: Mp4InputVideoDecoders,
    pub buffer: InputBufferOptions,
}

#[derive(Debug, Clone)]
pub struct Mp4OutputOptions {
    pub output_path: PathBuf,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
    pub raw_options: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum Mp4InputSource {
    Url(Arc<str>),
    File(Arc<Path>),
}

#[derive(Debug, Clone)]
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
