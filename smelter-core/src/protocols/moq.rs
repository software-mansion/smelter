use std::string::FromUtf8Error;
use std::sync::Arc;

use smelter_render::InputId;

use crate::codecs::VideoDecoderOptions;
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct MoqServerInputOptions {
    pub decoders: MoqServerInputDecoders,
    pub auth_token: Arc<str>,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoqServerInputDecoders {
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

    #[error("MoQ handshake failed: {0}")]
    MoqHandshakeFailed(#[source] anyhow::Error),
}
