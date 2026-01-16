use crate::{
    error::RtmpError,
    handshake::Handshake,
    message::{RtmpMessage, message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
    negotiation::negotiate_rtmp_session,
    protocol::MessageType,
    server::{OnConnectionCallback, RtmpAudioData, RtmpConnection, RtmpVideoData, ServerState},
};
use flv::{AudioTag, VideoTag};
use std::{
    net::TcpStream,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::channel},
};
use tracing::{info, trace};

pub(crate) fn handle_client(
    mut stream: TcpStream,
    _state: Arc<ServerState>,
    on_connection: Arc<Mutex<OnConnectionCallback>>,
) -> Result<(), RtmpError> {
    Handshake::perform(&mut stream)?;
    info!("Handshake complete");
    let mut message_writer = RtmpMessageWriter::new(stream.try_clone()?);
    let mut message_reader = RtmpMessageReader::new(stream, Arc::new(AtomicBool::new(false)));

    let (app, stream_key) = negotiate_rtmp_session(&mut message_reader, &mut message_writer)?;

    info!(?app, ?stream_key, "Negotiation complete");

    let (video_tx, video_rx) = channel();
    let (audio_tx, audio_rx) = channel();

    let connection_ctx = RtmpConnection {
        url_path: format!("/{app}/{stream_key}").into(),
        video_rx,
        audio_rx,
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
                let data = parse_audio(msg)?;
                audio_tx.send(data).map_err(|_| RtmpError::SocketClosed)?;
            }
            MessageType::Video => {
                let data = parse_video(msg)?;
                video_tx.send(data).map_err(|_| RtmpError::SocketClosed)?;
            }
            _ => {} // possible metadata
        }
        Ok(())
    })
}

fn parse_audio(msg: RtmpMessage) -> Result<RtmpAudioData, RtmpError> {
    let tag = AudioTag::parse(msg.payload)?;
    let dts = msg.timestamp as i64;
    Ok(RtmpAudioData {
        packet_type: tag.packet_type,
        pts: dts,
        dts,
        codec: tag.codec,
        sound_rate: tag.sound_rate,
        channels: tag.sound_type,
        data: tag.data,
    })
}

fn parse_video(msg: RtmpMessage) -> Result<RtmpVideoData, RtmpError> {
    let tag = VideoTag::parse(msg.payload)?;
    let dts = msg.timestamp as i64;
    let pts = tag.composition_time.map_or(dts, |cts| dts + cts as i64);
    Ok(RtmpVideoData {
        packet_type: tag.packet_type,
        pts,
        dts,
        codec: tag.codec,
        frame_type: tag.frame_type,
        composition_time: tag.composition_time,
        data: tag.data,
    })
}
