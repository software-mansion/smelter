use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use crate::{error::RtmpError, server::listen_thread::start_listener_thread};

mod connection;
mod event_channel;
mod listen_thread;
mod negotiation;

pub use event_channel::{
    RtmpEventBufferSnapshot, RtmpEventReceiver, RtmpEventSendError, RtmpEventSender,
    rtmp_event_channel,
};

pub type OnConnectionCallback = Box<dyn FnMut(RtmpConnection) + Send + 'static>;

pub struct RtmpConnection {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub receiver: RtmpEventReceiver,
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

impl RtmpServer {
    pub fn config(&self) -> ServerConfig {
        self.config.clone()
    }

    pub fn start(
        config: ServerConfig,
        on_connection: OnConnectionCallback,
    ) -> Result<Arc<Mutex<Self>>, RtmpError> {
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
