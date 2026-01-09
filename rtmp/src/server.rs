use crate::{error::RtmpError, handle_client::handle_client};
use bytes::Bytes;
use std::{
    collections::HashSet,
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex, RwLock, mpsc::Receiver},
    thread,
};
use tracing::{error, info};

pub type OnConnectionCallback = Box<dyn FnMut(RtmpConnection) + Send + 'static>;
pub type RtmpUrlPath = Arc<str>;

pub struct RtmpConnection {
    pub url_path: RtmpUrlPath,
    pub video_rx: Receiver<Bytes>, // replace Bytes with type containing parsed media
    pub audio_rx: Receiver<Bytes>,
}

#[allow(dead_code)] // TODO add SSL/TLS
pub struct ServerConfig {
    pub port: u16,
    pub use_ssl: bool,
    pub cert_file: Option<Arc<str>>,
    pub key_file: Option<Arc<str>>,
    pub ca_cert_file: Option<Arc<str>>,
    pub client_timeout_secs: u64,
}
#[allow(unused)]
pub(crate) struct ServerState {
    active_streams: RwLock<HashSet<RtmpUrlPath>>,
}

impl ServerState {
    fn new() -> Self {
        Self {
            active_streams: RwLock::new(HashSet::new()),
        }
    }

    #[allow(dead_code)]
    fn try_register(&self, conn: RtmpUrlPath) -> bool {
        let mut streams = self.active_streams.write().unwrap();
        if streams.contains(&conn) {
            return false;
        }
        streams.insert(conn);
        true
    }

    #[allow(dead_code)]
    fn unregister(&self, conn: &RtmpUrlPath) {
        let mut streams = self.active_streams.write().unwrap();
        streams.remove(conn);
    }
}

pub struct RtmpServer {
    config: ServerConfig,
    state: Arc<ServerState>,
    on_connection: Arc<Mutex<OnConnectionCallback>>,
}

impl RtmpServer {
    pub fn new(config: ServerConfig, on_connection: OnConnectionCallback) -> Self {
        Self {
            config,
            state: Arc::new(ServerState::new()),
            on_connection: Arc::new(Mutex::new(on_connection)),
        }
    }

    pub fn run(&self) -> Result<(), RtmpError> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        let listener = TcpListener::bind(addr)?;

        info!("RTMP server listening on port {}", self.config.port);

        for stream_result in listener.incoming() {
            match stream_result {
                Ok(stream) => {
                    let peer_addr = stream.peer_addr().ok();
                    info!("New connection from: {:?}", peer_addr);

                    let state = self.state.clone();
                    let on_connection = self.on_connection.clone();

                    thread::spawn(move || {
                        if let Err(error) = handle_client(stream, state, on_connection) {
                            error!(?error, "Client handler error");
                        }
                    });
                }
                Err(error) => {
                    error!(?error, "Accept error");
                }
            }
        }
        Ok(())
    }
}
