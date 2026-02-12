use std::{
    collections::HashMap,
    net::{SocketAddr, TcpStream},
    sync::{Arc, atomic::AtomicBool},
};

use tracing::{debug, info};

use crate::{
    AudioConfig, AudioData, AudioTag, PacketType, VideoConfig, VideoData, VideoTag,
    amf0::Amf0Value,
    error::RtmpError,
    flv::VideoFrameType,
    handshake::Handshake,
    message::RtmpMessage,
    protocol::{message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
};

pub struct RtmpClientConfig {
    pub addr: SocketAddr,
    pub app: String,
    pub stream_key: String,
}

pub struct RtmpClient {
    writer: RtmpMessageWriter,
    _reader: RtmpMessageReader,
    stream_id: u32,
}

impl RtmpClient {
    pub fn connect(config: RtmpClientConfig) -> Result<Self, RtmpError> {
        let stream = TcpStream::connect(config.addr)?;
        stream.set_nonblocking(false)?;

        let mut rw_stream = stream.try_clone()?;
        Handshake::perform_as_client(&mut rw_stream)?;
        info!("Client handshake complete");

        let mut writer = RtmpMessageWriter::new(stream.try_clone()?);
        let should_close = Arc::new(AtomicBool::new(false));
        let mut reader = RtmpMessageReader::new(stream, should_close);

        let stream_id =
            ConnectionNegotiation::run(&mut reader, &mut writer, &config.app, &config.stream_key)?;

        info!(stream_id, app = %config.app, stream_key = %config.stream_key, "RTMP client connected");

        Ok(Self {
            writer,
            _reader: reader,
            stream_id,
        })
    }

    pub fn send_video_config(&mut self, config: &VideoConfig) -> Result<(), RtmpError> {
        self.writer.write(RtmpMessage::Video {
            tag: VideoTag {
                packet_type: PacketType::Config,
                codec: config.codec,
                composition_time: None,
                frame_type: VideoFrameType::Keyframe,
                data: config.data.clone(),
            },
            timestamp: 0,
            stream_id: self.stream_id,
        })
    }

    pub fn send_audio_config(&mut self, config: &AudioConfig) -> Result<(), RtmpError> {
        self.writer.write(RtmpMessage::Audio {
            tag: AudioTag {
                packet_type: PacketType::Config,
                codec: config.codec,
                sample_rate: config.sample_rate,
                channels: config.channels,
                data: config.data.clone(),
            },
            timestamp: 0,
            stream_id: self.stream_id,
        })
    }

    pub fn send_video(&mut self, video: &VideoData) -> Result<(), RtmpError> {
        self.writer.write(RtmpMessage::Video {
            tag: VideoTag {
                packet_type: PacketType::Data,
                codec: video.codec,
                composition_time: video.composition_time,
                frame_type: video.frame_type,
                data: video.data.clone(),
            },
            timestamp: video.dts,
            stream_id: self.stream_id,
        })
    }

    pub fn send_audio(&mut self, audio: &AudioData) -> Result<(), RtmpError> {
        self.writer.write(RtmpMessage::Audio {
            tag: AudioTag {
                packet_type: PacketType::Data,
                codec: audio.codec,
                sample_rate: audio.sample_rate,
                channels: audio.channels,
                data: audio.data.clone(),
            },
            timestamp: audio.dts,
            stream_id: self.stream_id,
        })
    }
}

enum ConnectionNegotiation {
    WaitingForConnectResult,
    WaitingForCreateStreamResult,
    WaitingForPublishResult { stream_id: u32 },
}

impl ConnectionNegotiation {
    fn run(
        reader: &mut RtmpMessageReader,
        writer: &mut RtmpMessageWriter,
        app: &str,
        stream_key: &str,
    ) -> Result<u32, RtmpError> {
        send_connect(writer, app)?;
        let mut negotiation = ConnectionNegotiation::WaitingForConnectResult;

        loop {
            let msg = match reader.next() {
                Some(Ok(m)) => m,
                Some(Err(e)) => return Err(e),
                None => return Err(RtmpError::ChannelClosed),
            };

            match (&negotiation, &msg) {
                (
                    ConnectionNegotiation::WaitingForConnectResult,
                    RtmpMessage::CommandMessageAmf0 { values, .. },
                ) => {
                    if maybe_connect_result(values).is_some() {
                        negotiation = ConnectionNegotiation::WaitingForCreateStreamResult;
                        send_create_stream(writer)?;
                        continue;
                    };
                }
                (
                    ConnectionNegotiation::WaitingForCreateStreamResult,
                    RtmpMessage::CommandMessageAmf0 { values, .. },
                ) => {
                    if let Some(stream_id) = maybe_create_stream_result(values) {
                        negotiation = ConnectionNegotiation::WaitingForPublishResult { stream_id };
                        send_publish(writer, stream_key, stream_id)?;
                    };
                }
                (
                    ConnectionNegotiation::WaitingForPublishResult { stream_id },
                    RtmpMessage::CommandMessageAmf0 { values, .. },
                ) => {
                    if maybe_publish_result(values).is_some() {
                        return Ok(*stream_id);
                    }
                }
                _ => (),
            };

            Self::default_handler(msg, reader);
        }
    }

    fn default_handler(msg: RtmpMessage, reader: &mut RtmpMessageReader) {
        if let RtmpMessage::SetChunkSize { chunk_size } = msg {
            reader.set_chunk_size(chunk_size as usize);
            debug!(chunk_size, "Server set chunk size");
        }
    }
}

fn send_connect(writer: &mut RtmpMessageWriter, app: &str) -> Result<(), RtmpError> {
    // TODO: Investigate those values
    let props = HashMap::from([
        ("app".to_string(), Amf0Value::String(app.to_string())),
        (
            "flashVer".to_string(),
            Amf0Value::String("FMLE/3.0".to_string()),
        ),
        (
            "tcUrl".to_string(),
            Amf0Value::String(format!("rtmp://localhost/{app}")),
        ),
        ("fpad".to_string(), Amf0Value::Boolean(false)),
        ("capabilities".to_string(), Amf0Value::Number(15.0)),
        ("audioCodecs".to_string(), Amf0Value::Number(3191.0)),
        ("videoCodecs".to_string(), Amf0Value::Number(252.0)),
        ("videoFunction".to_string(), Amf0Value::Number(1.0)),
        ("objectEncoding".to_string(), Amf0Value::Number(0.0)),
    ]);

    writer.write(RtmpMessage::CommandMessageAmf0 {
        values: vec![
            Amf0Value::String("connect".to_string()),
            Amf0Value::Number(1.0),
            Amf0Value::Object(props),
        ],
        stream_id: 0,
    })
}

fn send_create_stream(writer: &mut RtmpMessageWriter) -> Result<(), RtmpError> {
    writer.write(RtmpMessage::CommandMessageAmf0 {
        values: vec![
            Amf0Value::String("createStream".to_string()),
            Amf0Value::Number(2.0),
            Amf0Value::Null,
        ],
        stream_id: 0,
    })
}

fn send_publish(
    writer: &mut RtmpMessageWriter,
    stream_key: &str,
    stream_id: u32,
) -> Result<(), RtmpError> {
    writer.write(RtmpMessage::CommandMessageAmf0 {
        values: vec![
            Amf0Value::String("publish".to_string()),
            Amf0Value::Number(0.0),
            Amf0Value::Null,
            Amf0Value::String(stream_key.to_string()),
            Amf0Value::String("live".to_string()),
        ],
        stream_id,
    })
}

fn maybe_connect_result(values: &[Amf0Value]) -> Option<()> {
    if let Some(Amf0Value::String(cmd)) = values.first()
        && cmd == "_result"
    {
        return Some(());
    }
    None
}

fn maybe_create_stream_result(values: &[Amf0Value]) -> Option<u32> {
    if let Some(Amf0Value::String(cmd)) = values.first()
        && cmd == "_result"
        && let Some(Amf0Value::Number(id)) = values.get(3)
    {
        return Some(*id as u32);
    }
    // TODO: maybe return Some(0)
    None
}

fn maybe_publish_result(values: &[Amf0Value]) -> Option<()> {
    if let Some(Amf0Value::String(cmd)) = values.first()
        && cmd == "onStatus"
    {
        return Some(());
    }
    None
}
