use crate::amf0::parser::AmfValue;
use crate::error::RtmpError;
use crate::handshake::Handshake;
use crate::header::ChunkMessageHeader;
use crate::messages::{RtmpMessage, parser::MessageParser};
use bytes::{BufMut, BytesMut};
use flv::{AudioCodec, Codec, PacketType, VideoCodec, parse_audio_payload, parse_video_payload};
use std::collections::HashMap;
use std::{net::SocketAddr, time::Duration};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::timeout,
};
use tracing::{debug, error, info, warn};

const DEFAULT_CHUNK_SIZE: u32 = 4096;
const WINDOW_ACK_SIZE: u32 = 2_500_000;

// TODO split into files

#[allow(dead_code)] // TODO add SSL/TLS
pub struct ServerConfig {
    pub port: u16,
    pub use_ssl: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_cert_file: Option<String>,
    pub client_timeout_secs: u64,
}

pub struct RtmpServer {
    config: ServerConfig,
    stream_tx: Option<mpsc::Sender<StreamEvent>>,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Connected {
        app: String,
        stream_key: String,
    },
    VideoConfig {
        codec: VideoCodec,
        config_data: Vec<u8>,
        timestamp: u32,
    },
    Video {
        codec: VideoCodec,
        payload: Vec<u8>,
        pts: i64,
        dts: i64,
        is_keyframe: bool,
    },
    AudioConfig {
        codec: AudioCodec,
        config_data: Vec<u8>,
        sample_rate: Option<u32>,
        channels: Option<flv::AudioChannels>,
        timestamp: u32,
    },
    Audio {
        codec: AudioCodec,
        payload: Vec<u8>,
        timestamp: u32,
        sample_rate: Option<u32>,
        channels: Option<flv::AudioChannels>,
    },
    Metadata {
        data: Vec<AmfValue>,
    },
    Disconnected,
}

impl RtmpServer {
    pub fn new(config: ServerConfig, sender: Option<mpsc::Sender<StreamEvent>>) -> Self {
        Self {
            config,
            stream_tx: sender,
        }
    }

    pub async fn run(&self) -> Result<(), RtmpError> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        let listener = TcpListener::bind(addr).await?;

        info!("RTMP server listening on port {}", self.config.port);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    info!("New connection from: {}", peer_addr);

                    let client_timeout = self.config.client_timeout_secs;
                    let stream_tx = self.stream_tx.clone();

                    tokio::spawn(async move {
                        let result = handle_client(stream, client_timeout, stream_tx).await;

                        if let Err(error) = result {
                            error!(?error, "Client handler error");
                        }
                    });
                }
                Err(error) => {
                    error!(?error, "Accept error");
                }
            }
        }
    }
}

struct SessionState {
    chunk_size: u32,
    app: String,
    stream_key: String,
    stream_id: u32,
    connected: bool,
    publishing: bool,
}

impl SessionState {
    fn new() -> Self {
        Self {
            chunk_size: 128,
            app: String::new(),
            stream_key: String::new(),
            stream_id: 1,
            connected: false,
            publishing: false,
        }
    }
}

async fn handle_client(
    mut stream: TcpStream, // TODO will propbably change when TLS/SSL added
    timeout_secs: u64,
    stream_tx: Option<mpsc::Sender<StreamEvent>>,
) -> Result<(), RtmpError> {
    let timeout_duration = Duration::from_secs(timeout_secs);

    timeout(timeout_duration, Handshake::perform(&mut stream))
        .await
        .map_err(|_| RtmpError::Timeout)??;

    info!("RTMP handshake completed");

    let mut parser = MessageParser::new();
    let mut buffer = vec![0u8; 65536];
    let mut data = Vec::new();
    let mut session = SessionState::new();

    loop {
        let n = match stream.read(&mut buffer).await {
            Ok(0) => {
                info!("Client disconnected");
                if let Some(tx) = &stream_tx {
                    let _ = tx.send(StreamEvent::Disconnected).await;
                }
                break;
            }
            Ok(n) => n,
            Err(error) => {
                return Err(error.into());
            }
        };

        data.extend_from_slice(&buffer[..n]);

        loop {
            match parser.parse(&data) {
                Ok((messages, consumed)) => {
                    if consumed == 0 {
                        break;
                    }
                    data.drain(..consumed);

                    for (header, message) in messages {
                        if let Some(responses) =
                            handle_message(&mut session, &mut parser, &header, &message, &stream_tx)
                                .await?
                        {
                            for (resp_header, resp_msg) in responses {
                                let serialized =
                                    serialize_message(&resp_header, &resp_msg, session.chunk_size);
                                stream.write_all(&serialized).await?;
                            }
                            stream.flush().await?;
                        }
                    }
                }
                Err(error) => {
                    warn!(?error, "Parse error");
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn handle_message(
    session: &mut SessionState,
    parser: &mut MessageParser,
    header: &ChunkMessageHeader,
    message: &RtmpMessage,
    stream_tx: &Option<mpsc::Sender<StreamEvent>>,
) -> Result<Option<Vec<(ChunkMessageHeader, RtmpMessage)>>, RtmpError> {
    debug!("Received message: {:?}", message);

    match message {
        RtmpMessage::SetChunkSize(size) => {
            debug!("Client set chunk size to {size}");
            parser.set_chunk_size(*size);
            Ok(None)
        }
        RtmpMessage::Command {
            name,
            transaction_id,
            data,
        } => handle_command(session, name, *transaction_id, data, stream_tx).await,
        RtmpMessage::DataMessage(values) => {
            if session.publishing
                && let Some(tx) = stream_tx
            {
                let _ = tx
                    .send(StreamEvent::Metadata {
                        data: values.clone(),
                    })
                    .await;
            }
            Ok(None)
        }
        RtmpMessage::Audio(data) => {
            if session.publishing
                && let Some(tx) = stream_tx
            {
                match parse_audio_payload(data) {
                    Ok((packet_type, Codec::Audio(codec), codec_params, payload)) => {
                        let event = match packet_type {
                            PacketType::AudioConfig => StreamEvent::AudioConfig {
                                codec,
                                config_data: payload.to_vec(),
                                sample_rate: codec_params.sound_rate,
                                channels: codec_params.sound_type,
                                timestamp: header.timestamp,
                            },
                            PacketType::Audio => StreamEvent::Audio {
                                codec,
                                payload: payload.to_vec(),
                                timestamp: header.timestamp,
                                sample_rate: codec_params.sound_rate,
                                channels: codec_params.sound_type,
                            },
                            _ => {
                                warn!("Unexpected audio packet type: {:?}", packet_type);
                                return Ok(None);
                            }
                        };
                        let _ = tx.send(event).await;
                    }
                    Ok((_, Codec::Video(_), _, _)) => {
                        warn!("Received video codec in audio message");
                    }
                    Err(e) => {
                        warn!("Failed to parse audio payload: {:?}", e);
                    }
                }
            }
            Ok(None)
        }
        RtmpMessage::Video(data) => {
            if session.publishing
                && let Some(tx) = stream_tx
            {
                match parse_video_payload(data) {
                    Ok((packet_type, Codec::Video(codec), codec_params, payload)) => {
                        let dts = header.timestamp as i64;
                        let pts = dts + codec_params.composition_time as i64;

                        let event = match packet_type {
                            PacketType::VideoConfig => StreamEvent::VideoConfig {
                                codec,
                                config_data: payload.to_vec(),
                                timestamp: header.timestamp,
                            },
                            PacketType::Video => StreamEvent::Video {
                                codec,
                                payload: payload.to_vec(),
                                pts,
                                dts,
                                is_keyframe: codec_params.key_frame.unwrap_or(false),
                            },
                            _ => {
                                warn!("Unexpected video packet type: {:?}", packet_type);
                                return Ok(None);
                            }
                        };
                        let _ = tx.send(event).await;
                    }
                    Ok((_, Codec::Audio(_), _, _)) => {
                        warn!("Received audio codec in video message");
                    }
                    Err(e) => {
                        warn!("Failed to parse video payload: {:?}", e);
                    }
                }
            }
            Ok(None)
        }
        RtmpMessage::WindowAcknowledgementSize(size) => {
            debug!("Window ack size: {}", size);
            Ok(None)
        }
        RtmpMessage::Acknowledgement(seq) => {
            debug!("Acknowledgement: {}", seq);
            Ok(None)
        }
        _ => {
            debug!("Unhandled message type");
            Ok(None)
        }
    }
}

async fn handle_command(
    session: &mut SessionState,
    name: &str,
    transaction_id: f64,
    data: &[AmfValue],
    stream_tx: &Option<mpsc::Sender<StreamEvent>>,
) -> Result<Option<Vec<(ChunkMessageHeader, RtmpMessage)>>, RtmpError> {
    match name {
        "connect" => {
            if let Some(AmfValue::Object(props)) = data.first()
                && let Some(AmfValue::String(app)) = props.get("app")
            {
                session.app = app.clone();
                info!("App: {}", app);
            }

            let mut responses = Vec::new();
            responses.push((
                ChunkMessageHeader::new(2, 0, 0),
                RtmpMessage::WindowAcknowledgementSize(WINDOW_ACK_SIZE),
            ));

            responses.push((
                ChunkMessageHeader::new(2, 0, 0),
                RtmpMessage::SetPeerBandwidth {
                    size: WINDOW_ACK_SIZE,
                    limit_type: 2,
                },
            ));

            responses.push((
                ChunkMessageHeader::new(2, 0, 0),
                RtmpMessage::SetChunkSize(DEFAULT_CHUNK_SIZE),
            ));
            session.chunk_size = DEFAULT_CHUNK_SIZE;

            let result = RtmpMessage::Command {
                name: "_result".into(),
                transaction_id: 1.0,
                data: vec![
                    AmfValue::Object(HashMap::from([
                        ("fmsVer".into(), AmfValue::String("FMS/3,0,1,123".into())),
                        ("capabilities".into(), AmfValue::Number(31.0)),
                    ])),
                    AmfValue::Object(HashMap::from([
                        ("level".into(), AmfValue::String("status".into())),
                        (
                            "code".into(),
                            AmfValue::String("NetConnection.Connect.Success".into()),
                        ),
                        (
                            "description".into(),
                            AmfValue::String("Connection succeeded.".into()),
                        ),
                        ("objectEncoding".into(), AmfValue::Number(0.0)),
                    ])),
                ],
            };
            responses.push((ChunkMessageHeader::new(3, 0, 0), result));

            session.connected = true;
            Ok(Some(responses))
        }
        "releaseStream" | "FCPublish" => {
            if let Some(AmfValue::String(key)) = data.get(1) {
                session.stream_key = key.clone();
            }
            Ok(None)
        }
        "createStream" => {
            let result = RtmpMessage::Command {
                name: "_result".into(),
                transaction_id,
                data: vec![AmfValue::Null, AmfValue::Number(session.stream_id as f64)],
            };
            Ok(Some(vec![(ChunkMessageHeader::new(3, 0, 0), result)]))
        }
        "publish" => {
            if let Some(AmfValue::String(key)) = data.first()
                && session.stream_key.is_empty()
            {
                session.stream_key = key.clone();
            }

            session.publishing = true;
            info!(
                "Publishing stream: app={}, key={}",
                session.app, session.stream_key
            );

            if let Some(tx) = stream_tx {
                let _ = tx
                    .send(StreamEvent::Connected {
                        app: session.app.clone(),
                        stream_key: session.stream_key.clone(),
                    })
                    .await;
            }

            let status = RtmpMessage::Command {
                name: "onStatus".into(),
                transaction_id: 0.0,
                data: vec![
                    AmfValue::Null,
                    AmfValue::Object(HashMap::from([
                        ("level".into(), AmfValue::String("status".into())),
                        (
                            "code".into(),
                            AmfValue::String("NetStream.Publish.Start".into()),
                        ),
                        (
                            "description".into(),
                            AmfValue::String(format!("{} is now pubished.", session.stream_key)),
                        ),
                        (
                            "details".into(),
                            AmfValue::String(session.stream_key.clone()),
                        ),
                    ])),
                ],
            };
            Ok(Some(vec![(
                ChunkMessageHeader::new(3, session.stream_id, 0),
                status,
            )]))
        }
        "FCUnpublish" | "deleteStream" => {
            session.publishing = false;
            if let Some(tx) = stream_tx {
                let _ = tx.send(StreamEvent::Disconnected).await;
            }
            Ok(None)
        }
        _ => {
            debug!("Unhandled command: {}", name);
            Ok(None)
        }
    }
}

fn serialize_message(
    header: &ChunkMessageHeader,
    message: &RtmpMessage,
    chunk_size: u32,
) -> Vec<u8> {
    let payload = message.serialize();
    let msg_type = message.type_id();

    let mut result = BytesMut::new();

    // write header type 0
    let fmt = 0u8;
    let cs_id = header.chunk_stream_id;

    if cs_id < 64 {
        result.put_u8((fmt << 6) | (cs_id as u8));
    } else if cs_id < 320 {
        result.put_u8(fmt << 6);
        result.put_u8((cs_id - 64) as u8);
    } else {
        result.put_u8((fmt << 6) | 1);
        let cs_id_minus_64 = cs_id - 64;
        result.put_u8((cs_id_minus_64 & 0xFF) as u8);
        result.put_u8(((cs_id_minus_64 >> 8) & 0xFF) as u8);
    }

    // timestamp (3 bytes)
    let timestamp = header.timestamp.min(0xFFFFFF);
    result.put_u8(((timestamp >> 16) & 0xFF) as u8);
    result.put_u8(((timestamp >> 8) & 0xFF) as u8);
    result.put_u8((timestamp & 0xFF) as u8);

    // message length (3 bytes)
    let len = payload.len();
    result.put_u8(((len >> 16) & 0xFF) as u8);
    result.put_u8(((len >> 8) & 0xFF) as u8);
    result.put_u8((len & 0xFF) as u8);

    // message type (1 byte)
    result.put_u8(msg_type);

    // message stream ID (4 bytes, little endian)
    result.put_u32_le(header.msg_stream_id);

    // extended timestamp if needed | not sure if needed in current implementation
    if header.timestamp >= 0xFFFFFF {
        result.put_u32(header.timestamp);
    }

    // write payload in chunks
    let mut offset = 0;
    while offset < payload.len() {
        if offset > 0 {
            // type 3 header for continuation chunks
            if cs_id < 64 {
                result.put_u8((3 << 6) | (cs_id as u8));
            } else if cs_id < 320 {
                result.put_u8(3 << 6);
                result.put_u8((cs_id - 64) as u8);
            } else {
                result.put_u8((3 << 6) | 1);
                let cs_id_minus_64 = cs_id - 64;
                result.put_u8((cs_id_minus_64 & 0xFF) as u8);
                result.put_u8(((cs_id_minus_64 >> 8) & 0xFF) as u8);
            }
        }

        let end = (offset + chunk_size as usize).min(payload.len());
        result.extend_from_slice(&payload[offset..end]);
        offset = end;
    }

    result.to_vec()
}
