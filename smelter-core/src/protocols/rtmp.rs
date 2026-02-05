use std::sync::Arc;

use smelter_render::InputId;

use crate::{
    InputBufferOptions,
    codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions},
};

#[derive(Debug, Clone)]
pub struct RtmpOutputOptions {
    pub url: Arc<str>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct FfmpegRtmpServerInputOptions {
    pub url: Arc<str>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub buffer: InputBufferOptions,
}

#[derive(Debug, Clone)]
pub struct RtmpServerInputOptions {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub buffer: InputBufferOptions,
}

#[derive(Debug, Clone)]
pub struct RtmpServerInputVideoDecoders {
    pub h264: Option<VideoDecoderOptions>,
}

#[derive(Debug, thiserror::Error)]
pub enum RtmpServerError {
    #[error("RTMP server is not running, cannot start RTMP input.")]
    ServerNotRunning,

    #[error("Not registered (app, stream_key) pair.")]
    InvalidAppStreamKeyPair,

    #[error("Pipeline context is unavailable.")]
    PipelineContextUnavailable,

    #[error("Input {0} not found.")]
    InputNotFound(InputId),

    #[error("Input {0} is already registered.")]
    InputAlreadyRegistered(InputId),
}
