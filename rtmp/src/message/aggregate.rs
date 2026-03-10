use bytes::Buf;

use crate::{RtmpMessageParseError, message::RtmpMessage, protocol::RawMessage};

/// Parse an Aggregate message (type 22).
///
/// The payload is a sequence of FLV-style tagged sub-messages:
///   1 byte  – message type ID
///   3 bytes – payload length (big-endian)
///
///   According to RTMP spec it should be in big endian order
///   FFmpeg is parsing this according to E.4.1 FLV spec
///   3 bytes – timestamp low 24 bits (big-endian)
///   1 byte  – timestamp high 8 bits (extends to 32-bit timestamp)
///
///   3 bytes – stream ID (little-endian, per FLV spec; typically ignored)
///   N bytes – sub-message payload
///   4 bytes – back pointer (size of the entire preceding entry incl. header)
///
/// Sub-messages inherit the aggregate message's `stream_id`. Offset is calculated based on first
/// timestamp and added to each timestamp value
pub fn parse_aggregate_message(msg: RawMessage) -> Result<Vec<RtmpMessage>, RtmpMessageParseError> {
    let mut messages = Vec::new();
    let mut offset = None;

    let mut payload = msg.payload;

    while payload.len() >= 11 {
        let header = payload.split_to(11);
        let type_id = header[0];
        // 3-byte big-endian payload length
        let data_size = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
        // 4-byte timestamp: 3 low bytes then 1 high byte
        // TODO: verify update description
        let timestamp = u32::from_be_bytes([header[7], header[4], header[5], header[6]]);
        // 3-byte stream ID (little-endian) – we use the aggregate's stream_id instead.

        let offset = *offset.get_or_insert_with(|| timestamp.saturating_sub(msg.timestamp));

        if payload.len() < data_size + 4 {
            return Err(RtmpMessageParseError::PayloadTooShort);
        }
        let msg_payload = payload.split_to(data_size);
        let _back_pointer = payload.get_u32();

        let raw = RawMessage {
            msg_type: type_id,
            stream_id: msg.stream_id,
            timestamp: timestamp + offset,
            payload: msg_payload,
            chunk_stream_id: msg.chunk_stream_id,
        };

        messages.push(RtmpMessage::from_raw(raw)?)
    }

    Ok(messages)
}
