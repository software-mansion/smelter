use std::{io, sync::Arc};

use smelter_render::InputId;
use url::Url;

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct RtmpOutputOptions {
    pub connection: RtmpConnectionOptions,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone, PartialEq)]
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
        let path_key = path_segments.next().unwrap_or("");
        let stream_key = match url.query() {
            Some(query) => format!("{path_key}?{query}"),
            None => path_key.to_string(),
        };

        Ok(RtmpConnectionOptions {
            host: host.to_string(),
            port,
            app,
            stream_key,
            use_tls,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RtmpServerInputOptions {
    pub stream_key: Arc<str>,
    pub decoders: RtmpServerInputDecoders,
    pub queue_options: QueueInputOptions,
}

#[derive(Debug, Clone, PartialEq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_key_keeps_query_string() {
        let options = RtmpConnectionOptions::from_url(
            "rtmps://live.example.com:443/rtmp/123456?s_bl=1&s_vt=ig&a=Ab6I",
        )
        .unwrap();

        assert_eq!(options.app, "rtmp");
        assert_eq!(options.stream_key, "123456?s_bl=1&s_vt=ig&a=Ab6I");
    }

    #[test]
    fn stream_key_without_query_string_is_unchanged() {
        let options =
            RtmpConnectionOptions::from_url("rtmp://live.example.com/app/FB-123-0-Abc").unwrap();

        assert_eq!(options.stream_key, "FB-123-0-Abc");
    }
}
