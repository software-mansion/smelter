use std::time::Duration;

use crate::codecs::VideoDecoderOptions;
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone)]
pub struct SrtInputOptions {
    pub port: u16,
    pub video: Option<SrtInputVideoOptions>,
    pub audio: Option<SrtInputAudioOptions>,
    pub queue_options: QueueInputOptions,
    pub offset: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct SrtInputVideoOptions {
    pub decoder: VideoDecoderOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrtInputAudioOptions {
    Aac,
}

#[derive(Debug, thiserror::Error)]
pub enum SrtInputError {
    #[error("SRT library error: {0}")]
    Srt(#[from] libsrt::Error),

    #[error("SRT input must have at least one of `video` or `audio` specified.")]
    NoVideoOrAudio,

    #[error("Failed to bind SRT listener to port {0}.")]
    Bind(u16, #[source] libsrt::Error),

    #[error("SRT input accepts only an H264 video decoder.")]
    InvalidVideoDecoder,
}
