use std::{sync::Arc, time::Duration};

use smelter_render::{InputId, OutputId};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone)]
pub struct SrtOutputOptions {
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
    pub encryption: Option<SrtOutputEncryption>,
}

#[derive(Debug, Clone)]
pub struct SrtOutputEncryption {
    pub passphrase: Arc<str>,
    pub key_length: SrtEncryptionKeyLength,
}

#[derive(Debug, Clone)]
pub struct SrtInputOptions {
    pub video: Option<SrtInputVideoOptions>,
    pub audio: Option<SrtInputAudioOptions>,
    pub queue_options: QueueInputOptions,
    pub offset: Option<Duration>,
    pub encryption: Option<SrtInputEncryption>,
}

#[derive(Debug, Clone)]
pub struct SrtInputEncryption {
    pub passphrase: Arc<str>,
    pub key_length: SrtEncryptionKeyLength,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrtEncryptionKeyLength {
    Aes128,
    Aes192,
    Aes256,
}

impl SrtEncryptionKeyLength {
    pub fn pbkeylen(self) -> i32 {
        match self {
            SrtEncryptionKeyLength::Aes128 => 16,
            SrtEncryptionKeyLength::Aes192 => 24,
            SrtEncryptionKeyLength::Aes256 => 32,
        }
    }
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

    #[error("SRT input accepts only an H264 video decoder.")]
    InvalidVideoDecoder,
}

#[derive(Debug, thiserror::Error)]
pub enum SrtServerError {
    #[error("SRT server is not running, cannot start SRT input.")]
    ServerNotRunning,

    #[error("Stream id \"{0}\" is not registered.")]
    NotRegisteredStreamId(Arc<str>),

    #[error("Input {0} not found.")]
    InputNotFound(InputId),

    #[error("Input {0} is already registered.")]
    InputAlreadyRegistered(InputId),

    #[error("Output {0} is already registered.")]
    OutputAlreadyRegistered(OutputId),

    #[error("Input {0} already has an active connection.")]
    ConnectionAlreadyActive(InputId),

    #[error("Stream id received from peer is not valid UTF-8.")]
    StreamIdNotUtf8,

    #[error("Stream id \"{0}\" is already used by another input.")]
    StreamIdAlreadyUsed(Arc<str>),
}

#[derive(Debug, thiserror::Error)]
pub enum SrtOutputError {
    #[error("SRT library error: {0}")]
    Srt(#[from] libsrt::Error),

    #[error("SRT output accepts only H264 video encoders.")]
    UnsupportedVideoCodec,

    #[error("SRT output accepts only AAC audio encoders.")]
    UnsupportedAudioCodec,
}
