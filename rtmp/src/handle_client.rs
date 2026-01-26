use std::{
    net::TcpStream,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::channel},
};

use flv::{AudioTag, VideoTag, tag::PacketType};
use tracing::{info, trace};

use crate::{
    error::RtmpError,
    handshake::Handshake,
    message::{RtmpMessage, message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
    negotiation::negotiate_rtmp_session,
    protocol::MessageType,
    server::{
        AudioConfig, AudioData, OnConnectionCallback, RtmpConnection, RtmpMediaData, VideoConfig,
        VideoData,
    },
};

pub(crate) fn handle_client(
    mut stream: TcpStream,
    on_connection: Arc<Mutex<OnConnectionCallback>>,
) -> Result<(), RtmpError> {
    Handshake::perform(&mut stream)?;
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

    // not sure where to break
    message_reader.try_for_each(|msg_result| {
        let msg = msg_result?;
        trace!(msg_type=?msg.msg_type, timestamp=msg.timestamp, "RTMP message received");

        match msg.msg_type {
            MessageType::Audio => {
                let audio_data = parse_audio(msg)?;
                sender
                    .send(audio_data)
                    .map_err(|_| RtmpError::ChannelClosed)?;
            }
            MessageType::Video => {
                let video_data = parse_video(msg)?;
                sender
                    .send(video_data)
                    .map_err(|_| RtmpError::ChannelClosed)?;
            }
            _ => {} // possible metadata
        }
        Ok(())
    })
}

fn parse_audio(msg: RtmpMessage) -> Result<RtmpMediaData, RtmpError> {
    let tag = AudioTag::parse(msg.payload)?;
    match tag.packet_type {
        PacketType::Config => Ok(RtmpMediaData::AudioConfig(AudioConfig {
            codec: tag.codec,
            sound_rate: tag.sound_rate,
            channels: tag.sound_type,
            data: tag.data,
        })),
        PacketType::Data => {
            let dts = msg.timestamp as i64;
            Ok(RtmpMediaData::Audio(AudioData {
                pts: dts,
                dts,
                codec: tag.codec,
                sound_rate: tag.sound_rate,
                channels: tag.sound_type,
                data: tag.data,
            }))
        }
    }
}

fn parse_video(msg: RtmpMessage) -> Result<RtmpMediaData, RtmpError> {
    let tag = VideoTag::parse(msg.payload)?;
    match tag.packet_type {
        PacketType::Config => Ok(RtmpMediaData::VideoConfig(VideoConfig {
            codec: tag.codec,
            data: tag.data,
        })),
        PacketType::Data => {
            let dts = msg.timestamp as i64;
            let pts = tag.composition_time.map_or(dts, |cts| dts + cts as i64);
            Ok(RtmpMediaData::Video(VideoData {
                pts,
                dts,
                codec: tag.codec,
                frame_type: tag.frame_type,
                composition_time: tag.composition_time,
                data: tag.data,
            }))
        }
    }
}
