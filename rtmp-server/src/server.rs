use crate::{error::RtmpError, handshake::Handshake};
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, RwLock,
        mpsc::{Receiver, channel},
    },
    thread,
};
use tracing::{error, info};

pub type OnConnectionCallback =
    Arc<dyn (Fn(&RtmpConnection, Receiver<Vec<u8>>, Receiver<Vec<u8>>) -> bool) + Send + Sync>;

#[allow(dead_code)] // TODO add SSL/TLS
pub struct ServerConfig {
    pub port: u16,
    pub use_ssl: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_cert_file: Option<String>,
    pub client_timeout_secs: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RtmpConnection {
    pub app: String,
    pub stream_key: String,
}

impl RtmpConnection {
    pub fn new(app: String, stream_key: String) -> Self {
        Self { app, stream_key }
    }
}

#[allow(unused)]
struct ServerState {
    active_streams: RwLock<HashSet<RtmpConnection>>,
}

impl ServerState {
    fn new() -> Self {
        Self {
            active_streams: RwLock::new(HashSet::new()),
        }
    }

    #[allow(dead_code)]
    fn try_register(&self, conn: RtmpConnection) -> bool {
        let mut streams = self.active_streams.write().unwrap();
        if streams.contains(&conn) {
            return false;
        }
        streams.insert(conn);
        true
    }

    #[allow(dead_code)]
    fn unregister(&self, conn: &RtmpConnection) {
        let mut streams = self.active_streams.write().unwrap();
        streams.remove(conn);
    }
}

pub struct RtmpServer {
    config: ServerConfig,
    state: Arc<ServerState>,
    on_connection: Option<OnConnectionCallback>,
}

impl RtmpServer {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            state: Arc::new(ServerState::new()),
            on_connection: None,
        }
    }

    pub fn on_connection<F>(&mut self, callback: F)
    where
        F: Fn(&RtmpConnection, Receiver<Vec<u8>>, Receiver<Vec<u8>>) -> bool
            + Send
            + Sync
            + 'static,
    {
        self.on_connection = Some(Arc::new(callback));
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
    on_connection: Option<OnConnectionCallback>,
) -> Result<(), RtmpError> {
    Handshake::perform(&mut stream)?;
    let mut session = RtmpSession::new(stream);
    info!("Handshake complete");

    // connect with rtmp amf0 messages

    // get app and stream_key
    // hardcoded for now
    let conn_info = RtmpConnection::new("app".into(), "stream_key".into());

    // check if another stream is not actively streaming on that route

    let (video_tx, video_rx) = channel();
    let (audio_tx, audio_rx) = channel();

    // client
    let authorized = if let Some(ref cb) = on_connection {
        cb(&conn_info, video_rx, audio_rx)
    } else {
        true
    };

    // client can reject connection
    if !authorized {
        info!("Connection rejected by callback: {:?}", conn_info);
        return Err(RtmpError::StreamNotRegistered);
    }

    // send rtmp publish message

    loop {
        let msg = session.read_next_message()?;
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

struct RtmpMessage {
    type_id: u8,
    payload: Vec<u8>,
}

// jsut schema of structs based of my previous implementation
#[allow(unused)]
struct ChunkHeader {
    timestamp: u32,
    length: u32,
    type_id: u8,
    stream_id: u32,
}

// jsut schema of structs based of my previous implementation
#[allow(unused)]
struct RtmpSession {
    stream: TcpStream,
    prev_headers: HashMap<u32, ChunkHeader>,
    partial_payloads: HashMap<u32, Vec<u8>>,
    chunk_size: usize,
}

impl RtmpSession {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            prev_headers: HashMap::new(),
            partial_payloads: HashMap::new(),
            chunk_size: 128, // Default RTMP chunk size
        }
    }

    fn read_next_message(&mut self) -> Result<RtmpMessage, RtmpError> {
        unimplemented!()
    }
}
