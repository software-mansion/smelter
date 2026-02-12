use std::{
    collections::HashMap,
    net::{SocketAddr, TcpStream},
    sync::{
        Arc,
        atomic::AtomicBool,
    },
};

use bytes::Bytes;
use tracing::{debug, info, trace};

use crate::{
    amf0::{Amf0Value, decode_amf0_values, encode_amf_values},
    client_handshake::ClientHandshake,
    error::RtmpError,
    flv::{AudioChannels, VideoFrameType},
    message::{RtmpMessage, message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
    protocol::MessageType,
    server::{AudioConfig, AudioData, RtmpEvent, VideoConfig, VideoData},
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
        ClientHandshake::perform(&mut rw_stream)?;
        info!("Client handshake complete");

        let mut writer = RtmpMessageWriter::new(stream.try_clone()?);
        let should_close = Arc::new(AtomicBool::new(false));
        let mut reader = RtmpMessageReader::new(stream, should_close);

        let stream_id = negotiate_client_session(
            &mut reader,
            &mut writer,
            &config.app,
            &config.stream_key,
        )?;

        info!(stream_id, app = %config.app, stream_key = %config.stream_key, "RTMP client connected");

        Ok(Self {
            writer,
            _reader: reader,
            stream_id,
        })
    }

    pub fn send_video_config(&mut self, config: &VideoConfig) -> Result<(), RtmpError> {
        let mut payload = Vec::new();
        payload.push((1 << 4) | 7);
        payload.push(0);
        payload.extend_from_slice(&[0, 0, 0]);
        payload.extend_from_slice(&config.data);

        let msg = RtmpMessage {
            msg_type: MessageType::Video,
            stream_id: self.stream_id,
            timestamp: 0,
            payload: Bytes::from(payload),
        };
        self.writer.write(&msg)
    }

    pub fn send_audio_config(&mut self, config: &AudioConfig) -> Result<(), RtmpError> {
        let mut payload = Vec::new();
        let sound_type: u8 = match config.channels {
            AudioChannels::Mono => 0,
            AudioChannels::Stereo => 1,
        };
        let sound_rate: u8 = match config.sound_rate {
            5500 => 0,
            11_000 => 1,
            22_050 => 2,
            44_100 => 3,
            _ => 3,
        };
        payload.push((10 << 4) | (sound_rate << 2) | (1 << 1) | sound_type);
        payload.push(0);
        payload.extend_from_slice(&config.data);

        let msg = RtmpMessage {
            msg_type: MessageType::Audio,
            stream_id: self.stream_id,
            timestamp: 0,
            payload: Bytes::from(payload),
        };
        self.writer.write(&msg)
    }

    pub fn send_video(&mut self, video: &VideoData) -> Result<(), RtmpError> {
        let mut payload = Vec::new();
        let frame_type: u8 = match video.frame_type {
            VideoFrameType::Keyframe => 1,
            VideoFrameType::Interframe => 2,
        };
        payload.push((frame_type << 4) | 7);
        payload.push(1);
        let cts = video.composition_time.unwrap_or(0);
        let cts_bytes = cts.to_be_bytes();
        payload.extend_from_slice(&cts_bytes[1..4]);
        payload.extend_from_slice(&video.data);

        let msg = RtmpMessage {
            msg_type: MessageType::Video,
            stream_id: self.stream_id,
            timestamp: video.dts as u32,
            payload: Bytes::from(payload),
        };
        self.writer.write(&msg)
    }

    pub fn send_audio(&mut self, audio: &AudioData) -> Result<(), RtmpError> {
        let mut payload = Vec::new();
        let sound_type: u8 = match audio.channels {
            AudioChannels::Mono => 0,
            AudioChannels::Stereo => 1,
        };
        let sound_rate: u8 = match audio.sound_rate {
            5500 => 0,
            11_000 => 1,
            22_050 => 2,
            44_100 => 3,
            _ => 3,
        };
        payload.push((10 << 4) | (sound_rate << 2) | (1 << 1) | sound_type);
        payload.push(1);
        payload.extend_from_slice(&audio.data);

        let msg = RtmpMessage {
            msg_type: MessageType::Audio,
            stream_id: self.stream_id,
            timestamp: audio.dts as u32,
            payload: Bytes::from(payload),
        };
        self.writer.write(&msg)
    }

    pub fn send_event(&mut self, event: &RtmpEvent) -> Result<(), RtmpError> {
        match event {
            RtmpEvent::VideoConfig(config) => self.send_video_config(config),
            RtmpEvent::AudioConfig(config) => self.send_audio_config(config),
            RtmpEvent::Video(video) => self.send_video(video),
            RtmpEvent::Audio(audio) => self.send_audio(audio),
            RtmpEvent::Metadata(metadata) => self.send_metadata(metadata),
        }
    }

    pub fn send_metadata(&mut self, metadata: &crate::flv::ScriptData) -> Result<(), RtmpError> {
        let amf_values: Vec<Amf0Value> = metadata
            .values
            .iter()
            .map(script_data_value_to_amf0)
            .collect();

        let msg = RtmpMessage {
            msg_type: MessageType::DataMessageAmf0,
            stream_id: self.stream_id,
            timestamp: 0,
            payload: encode_amf_values(&amf_values).unwrap_or_default(),
        };
        self.writer.write(&msg)
    }
}

fn script_data_value_to_amf0(value: &crate::flv::ScriptDataValue) -> Amf0Value {
    match value {
        crate::flv::ScriptDataValue::Number(n) => Amf0Value::Number(*n),
        crate::flv::ScriptDataValue::Boolean(b) => Amf0Value::Boolean(*b),
        crate::flv::ScriptDataValue::String(s) => Amf0Value::String(s.clone()),
        crate::flv::ScriptDataValue::Object(map) => Amf0Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), script_data_value_to_amf0(v)))
                .collect(),
        ),
        crate::flv::ScriptDataValue::Null => Amf0Value::Null,
        crate::flv::ScriptDataValue::Undefined => Amf0Value::Undefined,
        crate::flv::ScriptDataValue::EcmaArray(map) => Amf0Value::EcmaArray(
            map.iter()
                .map(|(k, v)| (k.clone(), script_data_value_to_amf0(v)))
                .collect(),
        ),
        crate::flv::ScriptDataValue::StrictArray(arr) => {
            Amf0Value::StrictArray(arr.iter().map(script_data_value_to_amf0).collect())
        }
        crate::flv::ScriptDataValue::Date {
            unix_time,
            timezone_offset,
        } => Amf0Value::Date {
            unix_time: *unix_time,
            timezone_offset: *timezone_offset,
        },
        crate::flv::ScriptDataValue::LongString(s) => Amf0Value::LongString(s.clone()),
        crate::flv::ScriptDataValue::TypedObject {
            class_name,
            properties,
        } => Amf0Value::TypedObject {
            class_name: class_name.clone(),
            properties: properties
                .iter()
                .map(|(k, v)| (k.clone(), script_data_value_to_amf0(v)))
                .collect(),
        },
    }
}

fn negotiate_client_session(
    reader: &mut RtmpMessageReader,
    writer: &mut RtmpMessageWriter,
    app: &str,
    stream_key: &str,
) -> Result<u32, RtmpError> {
    send_connect(writer, app)?;
    trace!("Sent connect command");

    wait_for_result(reader)?;
    trace!("Received connect _result");

    send_create_stream(writer)?;
    trace!("Sent createStream command");

    let stream_id = wait_for_create_stream_result(reader)?;
    trace!(stream_id, "Received createStream _result");

    send_publish(writer, stream_key, stream_id)?;
    trace!("Sent publish command");

    wait_for_publish_start(reader)?;
    trace!("Received publish onStatus");

    Ok(stream_id)
}

fn send_connect(writer: &mut RtmpMessageWriter, app: &str) -> Result<(), RtmpError> {
    let mut props = HashMap::new();
    props.insert("app".to_string(), Amf0Value::String(app.to_string()));
    props.insert(
        "flashVer".to_string(),
        Amf0Value::String("FMLE/3.0".to_string()),
    );
    props.insert(
        "tcUrl".to_string(),
        Amf0Value::String(format!("rtmp://localhost/{app}")),
    );
    props.insert("fpad".to_string(), Amf0Value::Boolean(false));
    props.insert("capabilities".to_string(), Amf0Value::Number(15.0));
    props.insert("audioCodecs".to_string(), Amf0Value::Number(3191.0));
    props.insert("videoCodecs".to_string(), Amf0Value::Number(252.0));
    props.insert("videoFunction".to_string(), Amf0Value::Number(1.0));
    props.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));

    let command = vec![
        Amf0Value::String("connect".to_string()),
        Amf0Value::Number(1.0),
        Amf0Value::Object(props),
    ];

    let msg = RtmpMessage {
        msg_type: MessageType::CommandMessageAmf0,
        stream_id: 0,
        timestamp: 0,
        payload: encode_amf_values(&command).unwrap_or_default(),
    };
    writer.write(&msg)
}

fn send_create_stream(writer: &mut RtmpMessageWriter) -> Result<(), RtmpError> {
    let command = vec![
        Amf0Value::String("createStream".to_string()),
        Amf0Value::Number(2.0),
        Amf0Value::Null,
    ];

    let msg = RtmpMessage {
        msg_type: MessageType::CommandMessageAmf0,
        stream_id: 0,
        timestamp: 0,
        payload: encode_amf_values(&command).unwrap_or_default(),
    };
    writer.write(&msg)
}

fn send_publish(
    writer: &mut RtmpMessageWriter,
    stream_key: &str,
    stream_id: u32,
) -> Result<(), RtmpError> {
    let command = vec![
        Amf0Value::String("publish".to_string()),
        Amf0Value::Number(0.0),
        Amf0Value::Null,
        Amf0Value::String(stream_key.to_string()),
        Amf0Value::String("live".to_string()),
    ];

    let msg = RtmpMessage {
        msg_type: MessageType::CommandMessageAmf0,
        stream_id,
        timestamp: 0,
        payload: encode_amf_values(&command).unwrap_or_default(),
    };
    writer.write(&msg)
}

fn wait_for_result(reader: &mut RtmpMessageReader) -> Result<(), RtmpError> {
    loop {
        let msg = match reader.next() {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(e),
            None => return Err(RtmpError::ChannelClosed),
        };

        if handle_protocol_message(&msg, reader)? {
            continue;
        }

        if msg.msg_type == MessageType::CommandMessageAmf0 {
            let args = decode_amf0_values(msg.payload).unwrap_or_default();
            if let Some(Amf0Value::String(cmd)) = args.first() {
                if cmd == "_result" {
                    return Ok(());
                }
            }
        }
    }
}

fn wait_for_create_stream_result(reader: &mut RtmpMessageReader) -> Result<u32, RtmpError> {
    loop {
        let msg = match reader.next() {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(e),
            None => return Err(RtmpError::ChannelClosed),
        };

        if handle_protocol_message(&msg, reader)? {
            continue;
        }

        if msg.msg_type == MessageType::CommandMessageAmf0 {
            let args = decode_amf0_values(msg.payload).unwrap_or_default();
            if let Some(Amf0Value::String(cmd)) = args.first() {
                if cmd == "_result" {
                    if let Some(Amf0Value::Number(id)) = args.get(3) {
                        return Ok(*id as u32);
                    }
                    return Ok(0);
                }
            }
        }
    }
}

fn wait_for_publish_start(reader: &mut RtmpMessageReader) -> Result<(), RtmpError> {
    loop {
        let msg = match reader.next() {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(e),
            None => return Err(RtmpError::ChannelClosed),
        };

        if handle_protocol_message(&msg, reader)? {
            continue;
        }

        if msg.msg_type == MessageType::CommandMessageAmf0 {
            let args = decode_amf0_values(msg.payload).unwrap_or_default();
            if let Some(Amf0Value::String(cmd)) = args.first() {
                if cmd == "onStatus" {
                    return Ok(());
                }
            }
        }
    }
}

fn handle_protocol_message(
    msg: &RtmpMessage,
    reader: &mut RtmpMessageReader,
) -> Result<bool, RtmpError> {
    match msg.msg_type {
        MessageType::SetChunkSize => {
            if msg.payload.len() >= 4 {
                let chunk_size = u32::from_be_bytes([
                    msg.payload[0],
                    msg.payload[1],
                    msg.payload[2],
                    msg.payload[3],
                ]) & 0x7F_FF_FF_FF;
                reader.set_chunk_size(chunk_size as usize);
                debug!(chunk_size, "Server set chunk size");
            }
            Ok(true)
        }
        MessageType::WindowAckSize
        | MessageType::SetPeerBandwidth
        | MessageType::Acknowledgement
        | MessageType::UserControl => Ok(true),
        _ => Ok(false),
    }
}
