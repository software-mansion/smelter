use std::{io, sync::Arc, time::Duration};

use smelter_render::InputId;
use url::Url;

use crate::{
    InputBufferOptions,
    codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions},
};

#[derive(Debug, Clone)]
pub struct RtmpOutputOptions {
    pub connection: RtmpConnectionOptions,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct RtmpConnectionOptions {
    pub host: String,
    pub port: u16,
    pub app: String,
    pub stream_key: String,
    pub use_tls: bool,
}

impl RtmpConnectionOptions {
    pub fn from_url(url: &str) -> Result<Self, RtmpConnectionUrlError> {
        let url = Url::parse(url)?;

        let use_tls = match url.scheme() {
            "rtmp" => false,
            "rtmps" => true,
            scheme => {
                return Err(RtmpConnectionUrlError::UnsupportedScheme(
                    scheme.to_string(),
                ));
            }
        };

        let Some(host) = url.host_str() else {
            return Err(RtmpConnectionUrlError::InvalidFormat);
        };

        let port = url.port().unwrap_or(match use_tls {
            true => 443,
            false => 1935,
        });

        let mut path_segments = url.path().trim_start_matches('/').splitn(2, '/');
        let app = path_segments.next().unwrap_or("").to_string();
        let stream_key = path_segments.next().unwrap_or("").to_string();

        Ok(RtmpConnectionOptions {
            host: host.to_string(),
            port,
            app,
            stream_key,
            use_tls,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RtmpServerInputOptions {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub decoders: RtmpServerInputDecoders,
    pub buffer: InputBufferOptions,
    pub required: bool,
    pub offset: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct RtmpServerInputDecoders {
    pub h264: Option<VideoDecoderOptions>,
}

#[derive(Debug, thiserror::Error)]
pub enum RtmpServerError {
    #[error("RTMP server is not running, cannot start RTMP input.")]
    ServerNotRunning,

    #[error("Not registered app, stream_key pair (app={app}, stream_key={stream_key})")]
    NotRegisteredAppStreamKeyPair { app: Arc<str>, stream_key: Arc<str> },

    #[error("Input {0} not found.")]
    InputNotFound(InputId),

    #[error("Input {0} is already registered.")]
    InputAlreadyRegistered(InputId),

    #[error("Input {0} already has an active connection.")]
    ConnectionAlreadyActive(InputId),
}

#[derive(Debug, thiserror::Error)]
pub enum RtmpClientError {
    #[error("Failed to establish RTMP connection")]
    RtmpNegotiationError(#[from] rtmp::RtmpConnectionError),

    #[error("RTMP connection failed")]
    RtmpStreamError(#[from] rtmp::RtmpStreamError),

    #[error("RTMP AAC config error")]
    AacConfigParseError(#[from] rtmp::AacConfigParseError),

    #[error("Failed to parse RTMP url")]
    RtmpConnectionError(#[from] RtmpConnectionUrlError),

    #[error("Missing H264 decoder config")]
    MissingH264DecoderConfig,

    #[error("Missing AAC decoder config")]
    MissingAacDecoderConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum RtmpConnectionUrlError {
    #[error(transparent)]
    ParsingError(#[from] url::ParseError),

    #[error("URL must have the format rtmp[s]://<HOST>[:<PORT>]/<APP>/<STREAM_KEY>")]
    InvalidFormat,

    #[error("Unsupported URL scheme \"{0}\", expected \"rtmp\" or \"rtmps\"")]
    UnsupportedScheme(String),

    #[error("Failed to resolve host address {0}.")]
    NoHostFound(String),

    #[error("Failed to resolve host: {0}")]
    HostLookupFailed(String, io::Error),
}
