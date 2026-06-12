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
    port: u16,
    tls: Option<TlsConfig>,
    video_codecs: Vec<RtmpVideoCodec>,
    audio_codecs: Vec<RtmpAudioCodec>,
}

impl RtmpServerConfig {
    /// Build a config with default options:
    /// - TLS: disabled
    /// - advertised video codecs: [H264, VP8, VP9]
    /// - advertised audio codecs: [AAC, Opus]
    pub fn new(port: u16) -> Self {
        Self {
            port,
            tls: None,
            video_codecs: vec![
                RtmpVideoCodec::H264,
                RtmpVideoCodec::Vp8,
                RtmpVideoCodec::Vp9,
            ],
            audio_codecs: vec![RtmpAudioCodec::Aac, RtmpAudioCodec::Opus],
        }
    }

    /// Enable TLS (RTMPS). Defaults to disabled.
    pub fn with_tls(mut self, tls: TlsConfig) -> Self {
        self.tls = Some(tls);
        self
    }

    /// Override the video codecs advertised to clients during `connect`.
    /// Defaults to [H264, VP8, VP9].
    pub fn with_video_codecs(mut self, video_codecs: Vec<RtmpVideoCodec>) -> Self {
        self.video_codecs = video_codecs;
        self
    }

    /// Override the audio codecs advertised to clients during `connect`.
    /// Defaults to [AAC, Opus].
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
