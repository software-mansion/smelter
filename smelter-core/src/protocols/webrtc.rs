use reqwest::{Method, StatusCode};
use std::sync::Arc;
use url::{ParseError, Url};

use crate::{
    InputBufferOptions,
    codecs::{AudioEncoderOptions, VideoEncoderOptions, WebrtcVideoDecoderOptions},
    error::{DecoderInitError, EncoderInitError},
};
#[derive(Debug, Clone)]
pub struct WhipInputOptions {
    pub video_preferences: Vec<WebrtcVideoDecoderOptions>,
    pub bearer_token: Option<Arc<str>>,
    pub endpoint_override: Option<Arc<str>>,
    pub buffer: InputBufferOptions,
}

#[derive(Debug, Clone)]
pub struct WhepInputOptions {
    pub video_preferences: Vec<WebrtcVideoDecoderOptions>,
    pub bearer_token: Option<Arc<str>>,
    pub endpoint_url: Arc<str>,
    pub buffer: InputBufferOptions,
}

#[derive(Debug, Clone)]
pub struct VideoWhipOptions {
    pub encoder_preferences: Vec<VideoEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct AudioWhipOptions {
    pub encoder_preferences: Vec<AudioEncoderOptions>,
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
