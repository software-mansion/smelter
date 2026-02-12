use std::{
    net::TcpStream,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::channel},
};

use tracing::{info, trace};

use crate::{
    AudioConfig, AudioData, VideoConfig, VideoData,
    error::RtmpError,
    flv::PacketType,
    handshake::Handshake,
    message::RtmpMessage,
    protocol::{message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
    server::{
        OnConnectionCallback, RtmpConnection, RtmpEvent, negotiation::negotiate_rtmp_session,
    },
};

pub(crate) fn handle_connection(
    mut stream: TcpStream,
    on_connection: Arc<Mutex<OnConnectionCallback>>,
) -> Result<(), RtmpError> {
    Handshake::perform_as_server(&mut stream)?;
    info!("Handshake complete");
    let mut message_writer = RtmpMessageWriter::new(stream.try_clone()?);
    let mut message_reader = RtmpMessageReader::new(stream, Arc::new(AtomicBool::new(false)));

    let (app, stream_key) = negotiate_rtmp_session(&mut message_reader, &mut message_writer)?;

    info!(?app, ?stream_key, "Negotiation complete");

    let (sender, receiver) = channel();

    let connection_ctx = RtmpConnection {
        app: app.into(),
        stream_key: stream_key.into(),
        receiver, // TODO instead of returning a receiver, return custom iterator that exposes buffer details
    };

    {
        let mut cb = on_connection.lock().unwrap();
        cb(connection_ctx);
    }

    for msg in message_reader {
        let msg = msg?;
        trace!(?msg, "RTMP message received");

        let event = match msg {
            RtmpMessage::Audio { tag, timestamp, .. } => match tag.packet_type {
                PacketType::Data => RtmpEvent::Audio(AudioData {
                    pts: timestamp,
                    dts: timestamp,
                    codec: tag.codec,
                    sample_rate: tag.sample_rate,
                    channels: tag.channels,
                    data: tag.data,
                }),
                PacketType::Config => RtmpEvent::AudioConfig(AudioConfig {
                    codec: tag.codec,
                    sample_rate: tag.sample_rate,
                    channels: tag.channels,
                    data: tag.data,
                }),
            },
            RtmpMessage::Video { tag, timestamp, .. } => match tag.packet_type {
                PacketType::Config => RtmpEvent::VideoConfig(VideoConfig {
                    codec: tag.codec,
                    data: tag.data,
                }),
                PacketType::Data => RtmpEvent::Video(VideoData {
                    pts: tag
                        .composition_time
                        .map_or(timestamp, |cts| timestamp + cts as i64),
                    dts: timestamp,
                    codec: tag.codec,
                    frame_type: tag.frame_type,
                    composition_time: tag.composition_time,
                    data: tag.data,
                }),
            },
            RtmpMessage::ScriptData(data) => RtmpEvent::Metadata(data),
            _ => continue,
        };

        sender.send(event).map_err(|_| RtmpError::ChannelClosed)?;
    }
    Ok(())
}
