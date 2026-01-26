use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    thread,
    time::Duration,
};

use bytes::Bytes;
use flv::{AudioChannels, AudioCodec, FrameType, VideoCodec};
use tracing::{error, info};

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
    pub config: ServerConfig,
    shutdown: Arc<AtomicBool>,
}

impl RtmpServer {
    pub fn start(
        config: ServerConfig,
        on_connection: OnConnectionCallback,
    ) -> Result<Arc<Mutex<Self>>, RtmpError> {
        let port = config.port;
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let on_connection = Arc::new(Mutex::new(on_connection));

        let shutdown = Arc::new(AtomicBool::new(false));
        let server = Arc::new(Mutex::new(Self { config, shutdown }));

        info!("RTMP server running on port {port}");

        let server_weak: Weak<Mutex<RtmpServer>> = Arc::downgrade(&server);

        thread::Builder::new()
            .name("RTMP server".to_string())
            .spawn(move || {
                loop {
                    let Some(server) = server_weak.upgrade() else {
                        break;
                    };

                    if server.lock().unwrap().shutdown.load(Ordering::Relaxed) {
                        break;
                    }

                    match listener.accept() {
                        Ok((stream, peer_addr)) => {
                            info!("New connection from: {peer_addr:?}");

                            let on_connection_clone = on_connection.clone();
                            thread::spawn(move || {
                                if let Err(err) = stream.set_nonblocking(false) {
                                    error!(?err, "Failed to set stream blocking");
                                    return;
                                }
                                if let Err(error) = handle_client(stream, on_connection_clone) {
                                    error!(?error, "Client handler error");
                                }
                            });
                        }
                        Err(err) if err.kind() == ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(50));
                        }
                        Err(error) => {
                            error!(?error, "Accept error");
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                }
            })?;

        Ok(server)
    }

    pub fn shutdown(self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
