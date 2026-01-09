use crate::amf0::encoder::Encoder;
use crate::amf0::parser::{AmfValue, Parser};
use crate::{
    error::RtmpError,
    handshake::Handshake,
    message::{RtmpMessage, message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
    server::{OnConnectionCallback, RtmpConnection, ServerState},
};
use std::{
    collections::HashMap,
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
            _ => {} // possible metadata
        }
    }

    Ok(())
}

fn negotiate_rtmp_session(
    reader: &mut RtmpMessageReader,
    writer: &mut RtmpMessageWriter,
) -> Result<(String, String), RtmpError> {
    let encoder = Encoder;
    let mut app_name = String::new();
    let current_stream_id = 1;

    loop {
        let msg = match reader.next() {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(e),
            None => return Err(RtmpError::SocketClosed),
        };

        // negotiation is only using Command messages
        if msg.type_id != 20 {
            continue;
        }

        let args = Parser::parse(&msg.payload).unwrap_or_default();
        if args.is_empty() {
            continue;
        }

        let cmd = match args.first() {
            Some(AmfValue::String(s)) => s.as_str(),
            _ => continue,
        };

        let txn_id = match args.get(1) {
            Some(AmfValue::Number(n)) => *n,
            _ => 0.0,
        };

        match cmd {
            "connect" => {
                if let Some(AmfValue::Object(map)) = args.get(2)
                    && let Some(AmfValue::String(app)) = map.get("app")
                {
                    app_name = app.clone();
                }
                // send window ack size, andwith and stream begin

                // _result - connect response
                let mut props = HashMap::new();
                props.insert(
                    "fmsVer".to_string(),
                    AmfValue::String("FMS/3,0,1,123".into()),
                );
                props.insert("capabilities".to_string(), AmfValue::Number(31.0));

                let mut info = HashMap::new();
                info.insert("level".to_string(), AmfValue::String("status".into()));
                info.insert(
                    "code".to_string(),
                    AmfValue::String("NetConnection.Connect.Success".into()),
                );
                info.insert(
                    "description".to_string(),
                    AmfValue::String("Connection succeeded.".into()),
                );

                let response = vec![
                    AmfValue::String("_result".to_string()),
                    AmfValue::Number(txn_id),
                    AmfValue::Object(props),
                    AmfValue::Object(info),
                ];
                // TODO examine
                let message = RtmpMessage {
                    type_id: 20,
                    stream_id: 0,
                    timestamp: 0,
                    payload: encoder.encode(&response).unwrap_or_default().into(),
                };
                writer.write(&message)?;
            }

            "createStream" => {
                let response = vec![
                    AmfValue::String("_result".to_string()),
                    AmfValue::Number(txn_id),
                    AmfValue::Null,
                    AmfValue::Number(current_stream_id as f64),
                ];
                // TODO examine
                let message = RtmpMessage {
                    type_id: 20,
                    stream_id: 0,
                    timestamp: 0,
                    payload: encoder.encode(&response).unwrap_or_default().into(),
                };
                writer.write(&message)?;
            }

            "publish" => {
                let stream_key = match args.get(3) {
                    Some(AmfValue::String(s)) => s.clone(),
                    _ => "default".to_string(),
                };
                let mut status_info = HashMap::new();
                status_info.insert("level".to_string(), AmfValue::String("status".into()));
                status_info.insert(
                    "code".to_string(),
                    AmfValue::String("NetStream.Publish.Start".into()),
                );
                status_info.insert(
                    "description".to_string(),
                    AmfValue::String(format!("Publishing {stream_key}")),
                );

                let response = vec![
                    AmfValue::String("onStatus".to_string()),
                    AmfValue::Number(0.0),
                    AmfValue::Null,
                    AmfValue::Object(status_info),
                ];
                // TODO examine
                let message = RtmpMessage {
                    type_id: 20,
                    stream_id: current_stream_id,
                    timestamp: 0,
                    payload: encoder.encode(&response).unwrap_or_default().into(),
                };
                writer.write(&message)?;

                return Ok((app_name, stream_key));
            }

            "releaseStream" | "FCPublish" | "FCUnpublish" => {
                // ignore
            }

            _ => {
                warn!("Unhandled command during negotiation: {}", cmd);
            }
        }
    }
}
