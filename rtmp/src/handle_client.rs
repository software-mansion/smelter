use crate::{
    error::RtmpError,
    handshake::Handshake,
    message::{message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
    negotiation::negotiate_rtmp_session,
    protocol::MessageType,
    server::{OnConnectionCallback, RtmpAudioData, RtmpConnection, RtmpVideoData, ServerState},
};
use flv::{AudioTag, VideoTag};
use std::{
    net::TcpStream,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc::channel},
};
use tracing::{error, info, trace, warn};

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

    for msg_result in message_reader {
        let msg = match msg_result {
            Ok(msg) => msg,
            Err(error) => {
                error!(?error, "Error reading RTMP message");
                break;
            }
        };

        trace!(msg_type=?msg.msg_type, timestamp=msg.timestamp, "RTMP message received");

        match msg.msg_type {
            MessageType::Audio => {
                let parsed = match AudioTag::parse(msg.payload) {
                    Ok(tag) => tag,
                    Err(error) => {
                        warn!(?error, "Failed to parse FLV audio tag");
                        continue;
                    }
                };

                let dts = msg.timestamp as i64;
                let pts = dts;

                let media = RtmpAudioData {
                    packet_type: parsed.packet_type,
                    pts,
                    dts,
                    codec: parsed.codec,
                    sound_rate: parsed.sound_rate,
                    channels: parsed.sound_type,
                    data: parsed.data,
                };

                if audio_tx.send(media).is_err() {
                    break;
                }
            }
            MessageType::Video => {
                let parsed = match VideoTag::parse(msg.payload) {
                    Ok(tag) => tag,
                    Err(error) => {
                        warn!(?error, "Failed to parse FLV video tag");
                        continue;
                    }
                };

                let dts = msg.timestamp as i64;
                let pts = parsed
                    .composition_time
                    .map(|cts| dts + (cts as i64))
                    .unwrap_or(dts);

                let media = RtmpVideoData {
                    packet_type: parsed.packet_type,
                    pts,
                    dts,
                    codec: parsed.codec,
                    frame_type: parsed.frame_type,
                    composition_time: parsed.composition_time,
                    data: parsed.data,
                };

                if video_tx.send(media).is_err() {
                    break;
                }
            }
            _ => {} // possible metadata
        }
    }

    Ok(())
}
