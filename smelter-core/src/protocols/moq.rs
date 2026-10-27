use std::sync::Arc;

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};
use crate::queue::QueueInputOptions;

use super::LiveInputBufferOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct MoqClientInputOptions {
    pub endpoint_url: Arc<str>,
    pub broadcast_path: Arc<str>,
    pub decoder_options: MoqInputDecoders,
    pub queue_options: QueueInputOptions,
    pub buffer: LiveInputBufferOptions,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoqInputDecoders {
    pub h264: Option<VideoDecoderOptions>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoqClientOutputOptions {
    pub endpoint_url: Arc<str>,
    pub broadcast_path: Arc<str>,
    pub container: MoqOutputContainer,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

/// Wire format used to carry encoded frames inside MoQ objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoqOutputContainer {
    /// Microsecond timestamp varint prefix followed by the raw codec payload.
    Legacy,
    /// Fragmented MP4. Each frame is a complete moof+mdat; the init segment is
    /// published in the catalog.
    Cmaf,
    /// Low Overhead Container.
    Loc,
}

impl std::fmt::Display for MoqOutputContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy => write!(f, "legacy"),
            Self::Cmaf => write!(f, "cmaf"),
            Self::Loc => write!(f, "loc"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MoqClientError {
    #[error("Invalid MoQ relay URL \"{0}\": {1}")]
    InvalidUrl(Arc<str>, #[source] url::ParseError),

    #[error("Unsupported MoQ relay URL scheme \"{0}\", only \"https\" is supported.")]
    InvalidScheme(String),

    #[error("Failed to initialize MoQ client: {0}")]
    ClientInitFailed(String),

    #[error("Failed to connect to MoQ relay: {0}")]
    ConnectFailed(String),

    #[error("MoQ relay refused a broadcast under path \"{0}\".")]
    PublishFailed(Arc<str>),

    #[error("Failed to create a MoQ broadcast: {0}")]
    BroadcastInitFailed(String),

    #[error("H264 encoder did not produce a decoder configuration record.")]
    MissingH264EncoderConfig,

    #[error("AAC encoder did not produce an AudioSpecificConfig.")]
    MissingAacEncoderConfig,

    #[error("Failed to build a CMAF init segment: {0}")]
    InitSegmentError(String),

    #[error("Unsupported audio sample rate: {0}")]
    UnsupportedSampleRate(u32),
}
