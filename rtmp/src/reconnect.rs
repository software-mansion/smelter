use std::collections::HashMap;

use url::Url;

use crate::amf0::AmfValue;

pub(crate) const RECONNECT_REQUEST_CODE: &str = "NetConnection.Connect.ReconnectRequest";

/// Try to parse an `onStatus` info object as a Reconnect Request.
/// Returns `Some` only when `code == NetConnection.Connect.ReconnectRequest`.
pub(crate) fn try_parse_reconnect_request(info: &AmfValue) -> Option<ReconnectRequest> {
    let map = match info {
        AmfValue::Object(map) => map,
        AmfValue::EcmaArray(map) => map,
        _ => return None,
    };
    if get_str(map, "code")? != RECONNECT_REQUEST_CODE {
        return None;
    }
    if get_str(map, "level") != Some("status") {
        return None;
    }
    Some(ReconnectRequest {
        tc_url: get_str(map, "tcUrl").map(str::to_string),
        description: get_str(map, "description").map(str::to_string),
    })
}

fn get_str<'a>(map: &'a HashMap<String, AmfValue>, key: &str) -> Option<&'a str> {
    match map.get(key)? {
        AmfValue::String(s) => Some(s.as_str()),
        _ => None,
    }
}

/// E-RTMP `NetConnection.Connect.ReconnectRequest` payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ReconnectRequest {
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
