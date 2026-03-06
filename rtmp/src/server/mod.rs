use std::sync::Arc;

use crate::{RtmpConnectionError, RtmpStreamError};

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
    pub client_timeout_secs: u64,
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
