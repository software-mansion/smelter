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
    app: Arc<str>,
    stream_key: Arc<str>,
    receiver: Receiver<RtmpEvent>,
}

impl RtmpConnection {
    pub fn app(&self) -> &Arc<str> {
        &self.app
    }

    pub fn stream_key(&self) -> &Arc<str> {
        &self.stream_key
    }
}

impl Iterator for &RtmpConnection {
    type Item = RtmpEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}

// TODO add SSL/TLS
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub use_ssl: bool,
    pub cert_file: Option<Arc<str>>,
    pub key_file: Option<Arc<str>>,
    pub ca_cert_file: Option<Arc<str>>,
    pub client_timeout_secs: u64,
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
