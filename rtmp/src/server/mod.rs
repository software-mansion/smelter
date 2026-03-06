use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::Receiver,
};

use crate::{
    RtmpConnectionError, RtmpEvent, RtmpStreamError, server::listen_thread::start_listener_thread,
};

mod connection;
mod listen_thread;
mod negotiation;

pub type OnConnectionCallback = Box<dyn FnMut(RtmpConnection) + Send + 'static>;

pub struct RtmpConnection {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub receiver: Receiver<RtmpEvent>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub tls: Option<TlsConfig>,
    pub client_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_file: Arc<str>,
    pub key_file: Arc<str>,
}

pub struct RtmpServer {
    config: ServerConfig,
    shutdown: Arc<AtomicBool>,
}

#[derive(thiserror::Error, Debug)]
pub(super) enum RtmpServerConnectionError {
    #[error("Failed to establish RTMP connection.")]
    NegotiationFailed(#[from] RtmpConnectionError),

    #[error("Connection failed")]
    ConnectionFailed(#[from] RtmpStreamError),
}

impl RtmpServer {
    pub fn config(&self) -> ServerConfig {
        self.config.clone()
    }

    pub fn start(
        config: ServerConfig,
        on_connection: OnConnectionCallback,
    ) -> Result<Arc<Mutex<Self>>, std::io::Error> {
        start_listener_thread(config, on_connection)
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

impl Drop for RtmpServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
