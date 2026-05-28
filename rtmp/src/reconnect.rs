use std::collections::HashMap;

use url::Url;

use crate::{
    amf0::AmfValue,
    message::{CONTROL_MESSAGE_STREAM_ID, CommandMessage, RtmpMessageIncoming},
};

pub(crate) const RECONNECT_REQUEST_CODE: &str = "NetConnection.Connect.ReconnectRequest";

/// Build the AMF Info Object for a `NetConnection.Connect.ReconnectRequest` onStatus.
pub(crate) fn build_reconnect_info_object(
    tc_url: Option<&str>,
    description: Option<&str>,
) -> HashMap<String, AmfValue> {
    let mut info = HashMap::from_iter([
        ("level".to_string(), AmfValue::String("status".to_string())),
        (
            "code".to_string(),
            AmfValue::String(RECONNECT_REQUEST_CODE.to_string()),
        ),
    ]);
    if let Some(tc_url) = tc_url {
        info.insert("tcUrl".to_string(), AmfValue::String(tc_url.to_string()));
    }
    if let Some(description) = description {
        info.insert(
            "description".to_string(),
            AmfValue::String(description.to_string()),
        );
    }
    info
}

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

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReconnectUrlError {
    #[error("Failed to parse current tcUrl: {0}")]
    InvalidCurrentTcUrl(url::ParseError),

    #[error("Failed to resolve reconnect tcUrl: {0}")]
    InvalidReconnectTcUrl(url::ParseError),
}

/// Resolve the target reconnect URL per E-RTMP v2 spec.
///
/// - request `None` → returns current.
/// - request absolute → returned as-is.
/// - request relative / protocol-relative / path-only → resolved against current.
pub(crate) fn resolve_reconnect_url(
    current_tc_url: &str,
    request_tc_url: Option<&str>,
) -> Result<String, ReconnectUrlError> {
    let current = Url::parse(current_tc_url).map_err(ReconnectUrlError::InvalidCurrentTcUrl)?;
    let Some(request) = request_tc_url else {
        return Ok(current.to_string());
    };
    let resolved = current
        .join(request)
        .map_err(ReconnectUrlError::InvalidReconnectTcUrl)?;
    Ok(resolved.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_returns_current() {
        let out = resolve_reconnect_url("rtmp://a.com:1935/app", None).unwrap();
        assert_eq!(out, "rtmp://a.com:1935/app");
    }

    #[test]
    fn absolute_replaces() {
        let out = resolve_reconnect_url("rtmp://a.com:1935/app", Some("rtmp://b.com/app")).unwrap();
        assert_eq!(out, "rtmp://b.com/app");
    }

    #[test]
    fn protocol_relative_keeps_scheme() {
        let out = resolve_reconnect_url("rtmp://a.com:1935/app", Some("//b.com/app")).unwrap();
        assert_eq!(out, "rtmp://b.com/app");
    }

    #[test]
    fn path_only_keeps_host_and_port() {
        let out = resolve_reconnect_url("rtmp://a.com:1935/app", Some("/other")).unwrap();
        assert_eq!(out, "rtmp://a.com:1935/other");
    }
}
