use std::string::FromUtf8Error;
use std::sync::Arc;

use smelter_render::InputId;

use crate::codecs::VideoDecoderOptions;
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct MoqServerInputOptions {
    pub decoders: MoqInputDecoders,
    pub auth_token: Arc<str>,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoqClientInputOptions {
    pub endpoint_url: Arc<str>,
    pub broadcast_path: Arc<str>,
    pub decoders: MoqInputDecoders,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoqInputDecoders {
    pub h264: Option<VideoDecoderOptions>,
}

#[derive(Debug, thiserror::Error)]
pub enum MoqServerError {
    #[error("MoQ server is not running, cannot start MoQ input.")]
    ServerNotRunning,

    #[error("Input {0} not found.")]
    InputNotFound(InputId),

    #[error("URL path \"{0}\" not found among registered inputs.")]
    PathNotFound(Arc<str>),

    #[error("Invalid authentication token for input \"{0}\"")]
    InvalidToken(InputId),

    #[error("Missing authentication token for input \"{0}\"")]
    MissingToken(InputId),

    #[error("Input {0} is already registered.")]
    InputAlreadyRegistered(InputId),

    #[error("Input {0} already has an active broadcast connection.")]
    BroadcastAlreadyActive(InputId),

    #[error("Unable to spawn broadcast handler, input queue was dropped.")]
    QueueDropped,

    #[error("Unable to extract URL from the request.")]
    UrlNotFound,

    #[error("Unable to decode URL path: {0}")]
    UrlDecodeFailed(#[from] FromUtf8Error),
}

#[derive(Debug, thiserror::Error)]
pub enum MoqClientError {
    #[error("Invalid MoQ relay URL \"{0}\": {1}")]
    InvalidUrl(Arc<str>, #[source] url::ParseError),

    // TODO: (@jbrs) check if this can be deleted and connect over the pure QUIC with moqt scheme
    #[error("Unsupported MoQ relay URL scheme \"{0}\", only \"https\" is supported.")]
    InvalidScheme(String),

    #[error("Failed to initialize MoQ client: {0}")]
    ClientInitFailed(#[source] anyhow::Error),

    #[error("Failed to connect to MoQ relay: {0}")]
    ConnectFailed(#[source] anyhow::Error),
}

impl MoqServerError {
    pub fn http_status_code(&self) -> u16 {
        match self {
            Self::UrlNotFound | Self::UrlDecodeFailed(_) => 400,
            Self::PathNotFound(_) | Self::InputNotFound(_) => 404,
            Self::MissingToken(_) | Self::InvalidToken(_) => 401,
            _ => 400,
        }
    }
}
