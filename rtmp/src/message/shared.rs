use bytes::{Buf, Bytes};

use crate::{AmfDecodingError, RtmpMessageParseError, message::RtmpMessage};

pub(crate) struct SharedObject {
    name: String,
    version: u32,
    persistent: bool,
    events: Vec<SharedObjectEvent>,
}

/// A single event entry inside a Shared Object message.
/// `data` is raw bytes whose encoding (AMF0 or AMF3) depends on the parent message type.
#[derive(Debug, Clone)]
pub(crate) struct SharedObjectEvent {
    /// Event type code (1 = Use, 2 = Release, 3 = RequestChange, 4 = Change,
    /// 5 = Success, 6 = SendMessage, 7 = Status, 8 = ClearData,
    /// 9 = DeleteData, 10 = RequestRemove, 11 = UseSuccess).
    pub event_type: u8,
    pub data: Bytes,
}

impl SharedObject {
    /// B
    ///   2 bytes  – object name length
    ///   N bytes  – object name (UTF-8)
    ///   4 bytes  – current version
    ///   4 bytes  – flags (bit 0 = persistent)
    ///   Repeated until end of payload:
    ///     1 byte  – event type
    ///     4 bytes – event data length
    ///     N bytes – event data (AMF0 or AMF3 encoded)
    fn parse(mut payload: Bytes) -> Result<RtmpMessage, RtmpMessageParseError> {
        if payload.len() < 2 {
            return Err(RtmpMessageParseError::PayloadTooShort);
        }
        let name_len = payload.get_u16() as usize;
        _ = payload.split_to(2);

        if payload.len() < name_len + 8 {
            return Err(RtmpMessageParseError::PayloadTooShort);
        }
        let name_bytes = payload.split_to(name_len);
        let name =
            String::from_utf8(name_bytes.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;

        let version = payload.get_u32();
        let flags = payload.get_u32();
        let persistent = flags & 1 != 0;

        let mut events = Vec::new();
        while payload.len() >= 5 {
            let event_type = payload.get_u8();
            let event_data_len = payload.get_u32() as usize;
            if payload.len() < event_data_len {
                break;
            }
            let data = payload.copy_to_bytes(event_data_len);
            events.push(SharedObjectEvent { event_type, data });
        }

        Ok(RtmpMessage::SetPeerBandwidth)

        if amf3 {

            Ok(RtmpMessage::SharedObjectAmf3 {
                name,
                version,
                persistent,
                events,
            })
        } else {
            Ok(RtmpMessage::SharedObjectAmf0 {
                name,
                version,
                persistent,
                events,
            })
        }
    }
}
