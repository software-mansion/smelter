use crate::{
    amf0::{AmfValue, decode_amf0_values, encode_amf_values},
    error::RtmpError,
    message::{RtmpMessage, message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
    protocol::{
        MessageType,
        control::{send_set_peer_bandwidth, send_stream_begin, send_window_ack_size},
    },
};
use std::collections::HashMap;
use tracing::{debug, trace, warn};

pub const WINDOW_ACK_SIZE: u32 = 2_500_000;
pub const PEER_BANDWIDTH: u32 = 2_500_000;

enum NegotiationStatus {
    InProgress,
    Completed { app: String, stream_key: String },
}

pub(crate) fn negotiate_rtmp_session(
    reader: &mut RtmpMessageReader,
    writer: &mut RtmpMessageWriter,
) -> Result<(String, String), RtmpError> {
    let mut app_name = String::new();
    let current_stream_id = 0;

    loop {
        let msg = match reader.next() {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(e),
            None => return Err(RtmpError::ChannelClosed),
        };

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
                    debug!(chunk_size, "Client set chunk size during negotiation");
                }
                continue;
            }
            MessageType::WindowAckSize | MessageType::Acknowledgement => {
                // handling is optional so leave for now
                continue;
            }
            MessageType::CommandMessageAmf0 => {
                match handle_command_message(msg, writer, &mut app_name, current_stream_id)? {
                    NegotiationStatus::InProgress => {}
                    NegotiationStatus::Completed { app, stream_key } => {
                        return Ok((app, stream_key));
                    }
                }
            }
            _ => continue,
        }
    }
}

// TODO(wkazmierczak) refator this function
fn handle_command_message(
    msg: RtmpMessage,
    writer: &mut RtmpMessageWriter,
    app_name: &mut String,
    current_stream_id: u32,
) -> Result<NegotiationStatus, RtmpError> {
    let args = decode_amf0_values(msg.payload).unwrap_or_default();
    if args.is_empty() {
        return Ok(NegotiationStatus::InProgress);
    }

    let cmd = match args.first() {
        Some(AmfValue::String(s)) => s.as_str(),
        _ => return Ok(NegotiationStatus::InProgress),
    };

    let txn_id = match args.get(1) {
        Some(AmfValue::Number(n)) => *n,
        _ => 0.0,
    };

    match cmd {
        // https://rtmp.veriskope.com/docs/spec/#7211connect
        "connect" => {
            if let Some(AmfValue::Object(map)) = args.get(2)
                && let Some(AmfValue::String(app)) = map.get("app")
            {
                *app_name = app.clone();
            }

            send_window_ack_size(writer, WINDOW_ACK_SIZE)?;
            // Limit Type for now hardcoded to 0 - Hard, other possible values 1 - Soft, 2 - Dynamic
            // https://rtmp.veriskope.com/docs/spec/#545set-peer-bandwidth-6
            send_set_peer_bandwidth(writer, PEER_BANDWIDTH, 0)?;
            send_stream_begin(writer, 0)?;

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
            info.insert(
                "objectEncoding".to_string(),
                AmfValue::Number(0.0), // AMF0 encoding
            );

            let response = vec![
                AmfValue::String("_result".to_string()),
                AmfValue::Number(txn_id),
                AmfValue::Object(props),
                AmfValue::Object(info),
            ];

            let message = RtmpMessage {
                msg_type: MessageType::CommandMessageAmf0,
                stream_id: 0,
                timestamp: 0,
                payload: encode_amf_values(&response).unwrap_or_default(),
            };
            writer.write(&message)?;
            trace!("Sent connect _result response");
        }

        "createStream" => {
            let response = vec![
                AmfValue::String("_result".to_string()),
                AmfValue::Number(txn_id),
                AmfValue::Null,
                AmfValue::Number(current_stream_id as f64),
            ];

            let message = RtmpMessage {
                msg_type: MessageType::CommandMessageAmf0,
                stream_id: 0,
                timestamp: 0,
                payload: encode_amf_values(&response).unwrap_or_default(),
            };
            writer.write(&message)?;
            trace!(stream_id = current_stream_id, "Sent createStream _result");

            send_stream_begin(writer, current_stream_id)?;
            trace!(
                stream_id = current_stream_id,
                "Sent Stream Begin for new stream"
            );
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

            let message = RtmpMessage {
                msg_type: MessageType::CommandMessageAmf0,
                stream_id: current_stream_id,
                timestamp: 0,
                payload: encode_amf_values(&response).unwrap_or_default(),
            };
            writer.write(&message)?;
            trace!(?stream_key, "Sent publish onStatus response");

            return Ok(NegotiationStatus::Completed {
                app: app_name.clone(),
                stream_key,
            });
        }
        _ => {
            warn!("Unhandled command during negotiation: {}", cmd);
        }
    }
    Ok(NegotiationStatus::InProgress)
}
