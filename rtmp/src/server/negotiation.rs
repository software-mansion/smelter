use crate::{
    amf0::Amf0Value,
    error::RtmpError,
    message::RtmpMessage,
    protocol::{message_reader::RtmpMessageReader, message_writer::RtmpMessageWriter},
};
use std::collections::HashMap;
use tracing::{debug, trace, warn};

pub const WINDOW_ACK_SIZE: u32 = 2_500_000;
pub const PEER_BANDWIDTH: u32 = 2_500_000;

// TODO: make sure that it is the only one valid negotation sequence
enum NegotiationState {
    // Waiting for `connect` command from client.
    WaitingForConnect,
    // `connect` handled, waiting for `createStream`.
    Connected { app: String },
    // `createStream` handled, waiting for `publish`.
    StreamCreated { app: String, stream_id: u32 },
}

pub(crate) fn negotiate_rtmp_session(
    reader: &mut RtmpMessageReader,
    writer: &mut RtmpMessageWriter,
) -> Result<(String, String), RtmpError> {
    let mut state = NegotiationState::WaitingForConnect;
    let mut next_stream_id: u32 = 1;

    loop {
        let msg = match reader.next() {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(e),
            None => return Err(RtmpError::ChannelClosed),
        };

        match msg {
            RtmpMessage::SetChunkSize { chunk_size } => {
                reader.set_chunk_size(chunk_size as usize);
                debug!(chunk_size, "Client set chunk size during negotiation");
            }
            RtmpMessage::WindowAckSize { window_size } => {
                debug!(
                    window_size,
                    "Client sent Window Acknowledgement Size during negotiation"
                );
            }
            RtmpMessage::CommandMessageAmf0 { values, .. }
            | RtmpMessage::CommandMessageAmf3 { values, .. } => {
                if let Some((app, stream_key)) =
                    handle_command_message(values, writer, &mut state, &mut next_stream_id)?
                {
                    return Ok((app, stream_key));
                }
            }
            _ => continue,
        }
    }
}

// TODO: This needs to be stateful
fn handle_command_message(
    args: Vec<Amf0Value>,
    writer: &mut RtmpMessageWriter,
    state: &mut NegotiationState,
    next_stream_id: &mut u32,
) -> Result<Option<(String, String)>, RtmpError> {
    if args.is_empty() {
        return Ok(None);
    }

    let cmd = match args.first() {
        Some(Amf0Value::String(s)) => s.as_str(),
        _ => return Ok(None),
    };

    let txn_id = match args.get(1) {
        Some(Amf0Value::Number(n)) => *n,
        _ => 0.0,
    };

    match cmd {
        // https://rtmp.veriskope.com/docs/spec/#7211connect
        "connect" => {
            if !matches!(state, NegotiationState::WaitingForConnect) {
                warn!("Received duplicate connect command, ignoring");
                return Ok(None);
            }

            let mut app_name = String::new();
            if let Some(Amf0Value::Object(map)) = args.get(2)
                && let Some(Amf0Value::String(app)) = map.get("app")
            {
                app_name = app.clone();
            }

            writer.write(RtmpMessage::WindowAckSize {
                window_size: WINDOW_ACK_SIZE,
            })?;
            // Limit Type for now hardcoded to 0 - Hard, other possible values 1 - Soft, 2 - Dynamic
            // https://rtmp.veriskope.com/docs/spec/#545set-peer-bandwidth-6
            writer.write(RtmpMessage::SetPeerBandwidth {
                bandwidth: PEER_BANDWIDTH,
                limit_type: 0,
            })?;
            writer.write(RtmpMessage::StreamBegin { stream_id: 0 })?;

            // _result - connect response
            let props = HashMap::from([
                (
                    "fmsVer".to_string(),
                    Amf0Value::String("FMS/3,0,1,123".into()),
                ),
                ("capabilities".to_string(), Amf0Value::Number(31.0)),
            ]);

            let info = HashMap::from([
                ("level".to_string(), Amf0Value::String("status".into())),
                (
                    "code".to_string(),
                    Amf0Value::String("NetConnection.Connect.Success".into()),
                ),
                (
                    "description".to_string(),
                    Amf0Value::String("Connection succeeded.".into()),
                ),
                (
                    "objectEncoding".to_string(),
                    Amf0Value::Number(0.0), // AMF0 encoding
                ),
            ]);

            writer.write(RtmpMessage::CommandMessageAmf0 {
                values: vec![
                    Amf0Value::String("_result".to_string()),
                    Amf0Value::Number(txn_id), // should be always 1 for connect response
                    Amf0Value::Object(props),
                    Amf0Value::Object(info),
                ],
                stream_id: 0,
            })?;
            trace!("Sent connect _result response");

            *state = NegotiationState::Connected { app: app_name };
        }

        "createStream" => {
            if !matches!(state, NegotiationState::Connected { .. }) {
                warn!("Received createStream in unexpected state, ignoring");
                return Ok(None);
            }

            let app = match std::mem::replace(state, NegotiationState::WaitingForConnect) {
                NegotiationState::Connected { app } => app,
                _ => unreachable!(),
            };

            // Allocate a non-zero stream ID (stream 0 is reserved for NetConnection)
            let stream_id = *next_stream_id;
            *next_stream_id += 1;

            writer.write(RtmpMessage::CommandMessageAmf0 {
                values: vec![
                    Amf0Value::String("_result".to_string()),
                    Amf0Value::Number(txn_id),
                    Amf0Value::Null,
                    Amf0Value::Number(stream_id as f64),
                ],
                stream_id: 0,
            })?;
            trace!(stream_id, "Sent createStream _result");

            writer.write(RtmpMessage::StreamBegin { stream_id })?;
            trace!(stream_id, "Sent Stream Begin for new stream");

            *state = NegotiationState::StreamCreated { app, stream_id };
        }

        "publish" => {
            let (app, stream_id) = match state {
                NegotiationState::StreamCreated { app, stream_id } => (app.clone(), *stream_id),
                _ => {
                    warn!("Received publish in unexpected state, ignoring");
                    return Ok(None);
                }
            };

            let stream_key = match args.get(3) {
                Some(Amf0Value::String(s)) => s.clone(),
                _ => "".to_string(),
            };

            let status_info = HashMap::from([
                ("level".to_string(), Amf0Value::String("status".into())),
                (
                    "code".to_string(),
                    Amf0Value::String("NetStream.Publish.Start".into()),
                ),
                (
                    "description".to_string(),
                    Amf0Value::String(format!("Publishing {stream_key}")),
                ),
            ]);

            writer.write(RtmpMessage::CommandMessageAmf0 {
                values: vec![
                    Amf0Value::String("onStatus".to_string()),
                    Amf0Value::Number(0.0),
                    Amf0Value::Null,
                    Amf0Value::Object(status_info),
                ],
                stream_id,
            })?;
            trace!("Sent publish onStatus response");

            return Ok(Some((app, stream_key)));
        }

        // Non-standard but commonly sent by clients (FFmpeg, OBS) before createStream.
        // Acknowledge with empty _result to avoid stalling the client.
        "releaseStream" | "FCPublish" => {
            trace!(cmd, "Received non-standard command, sending empty _result");
            writer.write(RtmpMessage::CommandMessageAmf0 {
                values: vec![
                    Amf0Value::String("_result".to_string()),
                    Amf0Value::Number(txn_id),
                    Amf0Value::Null,
                    Amf0Value::Null,
                ],
                stream_id: 0,
            })?;
        }
        _ => {
            warn!("Unhandled command during negotiation: {cmd}");
        }
    }
    Ok(None)
}
