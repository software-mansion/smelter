use std::{
    collections::HashSet,
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex, RwLock, mpsc::Receiver},
    thread,
    time::Duration,
};

use bytes::Bytes;
use flv::{AudioChannels, AudioCodec, FrameType, VideoCodec};
use tracing::{error, info, warn};

use crate::{error::RtmpError, handle_client::handle_client};

pub type OnConnectionCallback = Box<dyn FnMut(RtmpConnection) + Send + 'static>;
pub type RtmpUrlPath = Arc<str>;

pub enum RtmpMediaData {
    Video(VideoData),
    VideoConfig(VideoConfig),
    Audio(AudioData),
    AudioConfig(AudioConfig),
}

#[derive(Debug, Clone)]
pub struct AudioData {
    pub pts: i64,
    pub dts: i64,
    pub codec: AudioCodec,
    pub sound_rate: u32,
    pub channels: AudioChannels,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub codec: AudioCodec,
    pub sound_rate: u32,
    pub channels: AudioChannels,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct VideoData {
    pub pts: i64,
    pub dts: i64,
    pub codec: VideoCodec,
    pub frame_type: FrameType,
    pub composition_time: Option<i32>,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct VideoConfig {
    pub codec: VideoCodec,
    pub data: Bytes,
}

pub struct RtmpConnection {
    pub url_path: RtmpUrlPath,
    pub receiver: Receiver<RtmpMediaData>,
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
    listener: TcpListener,
}

impl RtmpServer {
    pub fn new(
        config: ServerConfig,
        on_connection: OnConnectionCallback,
    ) -> Result<Self, RtmpError> {
        let port = config.port;
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let mut last_error: Option<std::io::Error> = None;
        for _ in 0..5 {
            match TcpListener::bind(addr) {
                Ok(listener) => {
                    return Ok(Self {
                        config,
                        state: Arc::new(ServerState::new()),
                        on_connection: Arc::new(Mutex::new(on_connection)),
                        listener,
                    });
                }
                Err(err) => {
                    warn!("Failed to bind to port {port}. Retrying ...");
                    last_error = Some(err)
                }
            };
            thread::sleep(Duration::from_millis(1000));
        }
        Err(last_error.unwrap().into())
    }

    pub fn run(self) -> Result<(), RtmpError> {
        info!("RTMP server running on port {}", self.config.port);
        for stream_result in self.listener.incoming() {
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
