use crate::{error::RtmpError, handshake::Handshake, message_reader::RtmpMessageReader};
use bytes::Bytes;
use std::{
    collections::HashSet,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex, RwLock,
        mpsc::{Receiver, channel},
    },
    thread,
};
use tracing::{error, info, trace};
use url::Url;

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
struct ServerState {
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

fn handle_client(
    mut stream: TcpStream,
    _state: Arc<ServerState>, // later, based on state there will be check if route available
    on_connection: Arc<Mutex<OnConnectionCallback>>,
) -> Result<(), RtmpError> {
    Handshake::perform(&mut stream)?;
    let message_reader = RtmpMessageReader::new(stream);
    info!("Handshake complete");

    // connect with rtmp amf0 messages

    // get rtmp url from `tcUrl` field
    // hardcoded for now
    let rtmp_url = Url::parse("rtmp://127.0.0.1:1935/app/stream_key").unwrap();

    // check if another stream is not actively streaming on that route

    let (video_tx, video_rx) = channel();
    let (audio_tx, audio_rx) = channel();

    let connection_ctx = RtmpConnection {
        url_path: rtmp_url.path().into(),
        video_rx,
        audio_rx,
    };

    {
        let mut cb = on_connection.lock().unwrap();
        cb(connection_ctx);
    }

    // send rtmp publish message

    for msg_result in message_reader {
        let msg = match msg_result {
            Ok(msg) => msg,
            Err(error) => {
                error!(?error, "Error reading RTMP message");
                break;
            }
        };

        trace!(msg_type=?msg.type_id,  "RTMP message received");

        match msg.type_id {
            8 => {
                if audio_tx.send(msg.payload).is_err() {
                    break;
                }
            }
            9 => {
                if video_tx.send(msg.payload).is_err() {
                    break;
                }
            }
            _ => {}
        }
    }

    Ok(())
}
