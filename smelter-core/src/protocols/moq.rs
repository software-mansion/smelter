use std::sync::Arc;

use smelter_render::InputId;

use crate::codecs::{AudioDecoderOptions, VideoDecoderOptions};
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct MoqServerInputOptions {
    pub broadcast_path: Arc<str>,
    pub decoders: MoqServerInputDecoders,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoqServerInputDecoders {
    pub h264: Option<VideoDecoderOptions>,
    pub aac: Option<AudioDecoderOptions>,
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
