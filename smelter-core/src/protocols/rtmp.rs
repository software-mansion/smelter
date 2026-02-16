use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

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
    pub address: SocketAddr,
    pub app: String,
    pub stream_key: String,
}

impl RtmpConnectionOptions {
    pub fn from_url(url: &str) -> Result<Self, RtmpConnectionUrlError> {
        let url = Url::parse(url)?;
        let Some(host) = url.host_str() else {
            return Err(RtmpConnectionUrlError::InvalidFormat);
        };
        let port = url.port().unwrap_or(1935);
        let Some(address) = (host, port)
            .to_socket_addrs()
            .map_err(|err| RtmpConnectionUrlError::HostLookupFailed(host.to_string(), err))?
            .next()
        else {
            return Err(RtmpConnectionUrlError::NoHostFound(host.to_string()));
        };

        let path_segments: Vec<&str> = url.path().trim_start_matches('/').splitn(2, '/').collect();
        let [app, stream_key] = &path_segments[..] else {
            return Err(RtmpConnectionUrlError::InvalidFormat);
        };

        Ok(RtmpConnectionOptions {
            address,
            app: app.to_string(),
            stream_key: stream_key.to_string(),
        })
    }
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
    #[error(transparent)]
    RtmpError(#[from] rtmp::RtmpError),

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

    #[error("URL needs have following format rtmp://<IP>:<PORT>/<APP>/<STREAM_KEY>")]
    InvalidFormat,

    #[error("Failed to resolve host address {0}.")]
    NoHostFound(String),

    #[error("Failed to resolve host: {0}")]
    HostLookupFailed(String, io::Error),
}
