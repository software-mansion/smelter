use crate::amf0::AmfValue;
use crate::message::{CONTROL_MESSAGE_STREAM_ID, CommandMessage, RtmpMessageIncoming};

pub(crate) const RECONNECT_REQUEST_CODE: &str = "NetConnection.Connect.ReconnectRequest";

pub(crate) fn try_match_reconnect_request(msg: &RtmpMessageIncoming) -> Option<ReconnectRequest> {
    let RtmpMessageIncoming::CommandMessage {
        msg: CommandMessage::OnStatus(info),
        stream_id: CONTROL_MESSAGE_STREAM_ID,
    } = msg
    else {
        return None;
    };

    let map = match info {
        AmfValue::Object(map) | AmfValue::EcmaArray(map) => map,
        _ => return None,
    };

    let code = map.get("code").and_then(AmfValue::as_str);
    let level = map.get("level").and_then(AmfValue::as_str);
    let tc_url = map.get("tcUrl").and_then(AmfValue::as_str);
    let description = map.get("description").and_then(AmfValue::as_str);

    if code != Some(RECONNECT_REQUEST_CODE) || level != Some("status") {
        return None;
    }

    Some(ReconnectRequest {
        tc_url: tc_url.map(str::to_string),
        description: description.map(str::to_string),
    })
}

/// E-RTMP `NetConnection.Connect.ReconnectRequest` payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReconnectRequest {
    /// Optional target URL the client should reconnect to.
    /// Absent: reconnect to current tcUrl.
    /// Relative reference: resolve against current tcUrl.
    /// Absolute: use as-is.
    pub tc_url: Option<String>,
    pub description: Option<String>,
}
