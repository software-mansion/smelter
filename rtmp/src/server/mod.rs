use std::sync::Arc;

use crate::{RtmpAudioCodec, RtmpConnectionError, RtmpStreamError, RtmpVideoCodec};

mod connection;
mod connection_thread;
mod instance;
mod listener_thread;
mod negotiation;

pub use connection::RtmpServerConnection;
pub use instance::RtmpServer;

pub type OnConnectionCallback = Box<dyn FnMut(RtmpServerConnection) + Send + 'static>;

#[derive(Debug, Clone)]
pub struct RtmpServerConfig {
    pub port: u16,
    pub tls: Option<TlsConfig>,
    /// Video codecs advertised to clients during `connect`. Defaults to all
    /// supported codecs when constructed via [`RtmpServerConfig::new`].
    pub video_codecs: Vec<RtmpVideoCodec>,
    /// Audio codecs advertised to clients during `connect`. Defaults to all
    /// supported codecs when constructed via [`RtmpServerConfig::new`].
    pub audio_codecs: Vec<RtmpAudioCodec>,
}

impl RtmpServerConfig {
    /// Build a config with all known codecs advertised.
    pub fn new(port: u16, tls: Option<TlsConfig>) -> Self {
        Self {
            port,
            tls,
            video_codecs: vec![RtmpVideoCodec::H264, RtmpVideoCodec::Vp8, RtmpVideoCodec::Vp9],
            audio_codecs: vec![RtmpAudioCodec::Aac, RtmpAudioCodec::Opus],
        }
    }

    /// Override the video codecs advertised to clients during `connect`.
    pub fn with_video_codecs(mut self, video_codecs: Vec<RtmpVideoCodec>) -> Self {
        self.video_codecs = video_codecs;
        self
    }

    /// Override the audio codecs advertised to clients during `connect`.
    pub fn with_audio_codecs(mut self, audio_codecs: Vec<RtmpAudioCodec>) -> Self {
        self.audio_codecs = audio_codecs;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_file: Arc<str>,
    pub key_file: Arc<str>,
}

#[derive(thiserror::Error, Debug)]
pub(super) enum RtmpServerConnectionError {
    #[error("Failed to establish RTMP connection.")]
    NegotiationFailed(#[from] RtmpConnectionError),

    #[error("Connection failed")]
    ConnectionFailed(#[from] RtmpStreamError),

    #[error("Received connection during RTMP server shutdown")]
    ShutdownInProgress,
}
