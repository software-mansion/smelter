use std::sync::Arc;

use smelter_render::InputId;

use crate::codecs::{AudioDecoderOptions, VideoDecoderOptions};
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct MoqServerInputOptions {
    pub broadcast_path: Arc<str>,
    pub decoders: MoqInputDecoders,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoqInputDecoders {
    pub h264: Option<VideoDecoderOptions>,
    pub aac: Option<AudioDecoderOptions>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoqClientInputOptions {
    pub url: Arc<str>,
    pub broadcast_path: Arc<str>,
    pub verify_tls: bool,
    pub decoders: MoqInputDecoders,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, thiserror::Error)]
pub enum MoqServerError {
    #[error("MoQ server is not running, cannot start MoQ input.")]
    ServerNotRunning,

    #[error("Input {0} not found.")]
    InputNotFound(InputId),

    #[error("Input {0} is already registered.")]
    InputAlreadyRegistered(InputId),

    #[error("Input {0} already has an active broadcast connection.")]
    BroadcastAlreadyActive(InputId),

    #[error("Broadcast path \"{broadcast_path}\" is already used by input \"{existing_input}\".")]
    BroadcastPathAlreadyUsed {
        broadcast_path: Arc<str>,
        existing_input: InputId,
    },

    #[error("Broadcast path \"{0}\" not found among registered inputs.")]
    BroadcastPathNotFound(Arc<str>),
}

#[derive(Debug, thiserror::Error)]
pub enum MoqClientError {
    #[error("Failed to parse URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Failed to connect to MoQ relay: {0}")]
    ConnectionError(#[source] anyhow::Error),

    #[error("Failed to initialize MoQ client: {0}")]
    ClientInitError(#[source] anyhow::Error),

    #[error("Broadcast not found on the MoQ relay.")]
    BroadcastNotFound,

    #[error("MoQ relay closed without announcing the requested broadcast.")]
    BroadcastNotAnnounced,
}
