use reqwest::{Method, StatusCode};
use smelter_render::Resolution;
use std::sync::Arc;
use url::{ParseError, Url};

use crate::{
    AudioChannels,
    codecs::{
        AudioEncoderOptions, FfmpegH264EncoderOptions, FfmpegVp8EncoderOptions,
        FfmpegVp9EncoderOptions, OpusEncoderOptions, VideoEncoderOptions, VulkanH264EncoderOptions,
    },
    error::{DecoderInitError, EncoderInitError},
    protocols::RtpJitterBufferOptions,
};

#[derive(Debug, Clone)]
pub struct WhipInputOptions {
    pub video_preferences: Vec<WebrtcVideoDecoderOptions>,
    pub bearer_token: Option<Arc<str>>,
    pub endpoint_override: Option<Arc<str>>,
    pub jitter_buffer: RtpJitterBufferOptions,
}

#[derive(Debug, Clone)]
pub struct WhepInputOptions {
    pub video_preferences: Vec<WebrtcVideoDecoderOptions>,
    pub bearer_token: Option<Arc<str>>,
    pub endpoint_url: Arc<str>,
    pub jitter_buffer: RtpJitterBufferOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebrtcVideoDecoderOptions {
    FfmpegH264,
    FfmpegVp8,
    FfmpegVp9,
    VulkanH264,
    Any,
}

#[derive(Debug, Clone)]
pub struct WhipOutputOptions {
    pub endpoint_url: Arc<str>,
    pub bearer_token: Option<Arc<str>>,
    pub video: Option<VideoWhipOptions>,
    pub audio: Option<AudioWhipOptions>,
}

#[derive(Debug, Clone)]
pub struct WhepOutputOptions {
    pub bearer_token: Option<Arc<str>>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct VideoWhipOptions {
    pub encoder_preferences: Vec<WhipVideoEncoderOptions>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum WhipVideoEncoderOptions {
    FfmpegH264(FfmpegH264EncoderOptions),
    FfmpegVp8(FfmpegVp8EncoderOptions),
    FfmpegVp9(FfmpegVp9EncoderOptions),
    VulkanH264(VulkanH264EncoderOptions),
    Any(Resolution),
}

#[derive(Debug, Clone)]
pub struct AudioWhipOptions {
    pub encoder_preferences: Vec<WhipAudioEncoderOptions>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum WhipAudioEncoderOptions {
    Opus(OpusEncoderOptions),
    Any(AudioChannels),
}

#[derive(Debug, thiserror::Error)]
pub enum WebrtcServerError {
    #[error("Endpoint ID already in use (endpoint_id: {0})")]
    EndpointIdAlreadyInUse(Arc<str>),

    #[error("WHIP/WHEP server is not running, cannot start WHIP input.")]
    ServerNotRunning,
}

#[derive(Debug, thiserror::Error)]
pub enum WebrtcClientError {
    #[error("Establishing the connection timed out")]
    Timeout,

    #[error("Bad status in response Status: {0} Body:\n{1}")]
    BadStatus(StatusCode, String),

    #[error("Request failed! Method: {0} URL: {1}")]
    RequestFailed(Method, Url),

    #[error(
        "Unable to get location endpoint, check correctness of webrtc endpoint and your bearer token"
    )]
    MissingLocationHeader,

    #[error("Invalid endpoint URL: {1}")]
    InvalidEndpointUrl(#[source] ParseError, String),

    #[error("Failed to create RTC session description: {0}")]
    RTCSessionDescriptionError(webrtc::Error),

    #[error("Failed to set local description: {0}")]
    LocalDescriptionError(webrtc::Error),

    #[error("Failed to set remote description: {0}")]
    RemoteDescriptionError(webrtc::Error),

    #[error("Failed to parse {0} response body: {1}")]
    BodyParsingError(&'static str, reqwest::Error),

    #[error("Failed to create offer: {0}")]
    OfferCreationError(webrtc::Error),

    #[error(transparent)]
    PeerConnectionInitError(#[from] webrtc::Error),

    #[error("Trickle ICE not supported")]
    TrickleIceNotSupported,

    #[error("Entity Tag missing")]
    EntityTagMissing,

    #[error("Entity Tag non-matching")]
    EntityTagNonMatching,

    #[error("No video codec was negotiated")]
    NoVideoCodecNegotiated,

    #[error("No audio codec was negotiated")]
    NoAudioCodecNegotiated,

    #[error("Codec not supported: {0}")]
    UnsupportedCodec(&'static str),

    #[error("Failed to initialize the decoder")]
    DecoderInitError(#[from] DecoderInitError),

    #[error("Failed to initialize the encoder")]
    EncoderInitError(#[from] EncoderInitError),
}
