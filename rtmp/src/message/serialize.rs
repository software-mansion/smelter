use bytes::{BufMut, Bytes, BytesMut};

use crate::{
    SerializationError,
    amf0::{encode_amf0_values, encode_avmplus_values},
    message::{RtmpMessage, UserControlEvent, event::event_into_raw},
    protocol::{MessageType, RawMessage},
};

impl RtmpMessage {
    pub fn into_raw(self) -> Result<RawMessage, SerializationError> {
        let result = match self {
            RtmpMessage::SetChunkSize { chunk_size } => RawMessage {
                msg_type: MessageType::SetChunkSize,
                stream_id: 0, // TODO: not sure if zero
                timestamp: 0,
                payload: Bytes::copy_from_slice(&chunk_size.to_be_bytes()),
            },
            RtmpMessage::AbortMessage { chunk_stream_id } => RawMessage {
                msg_type: MessageType::AbortMessage,
                stream_id: 0,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&chunk_stream_id.to_be_bytes()),
            },
            RtmpMessage::Acknowledgement { sequence_number } => RawMessage {
                msg_type: MessageType::Acknowledgement,
                stream_id: 0,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&sequence_number.to_be_bytes()),
            },
            RtmpMessage::UserControl { event } => serialize_user_control(event),
            RtmpMessage::WindowAckSize { window_size } => RawMessage {
                msg_type: MessageType::WindowAckSize,
                stream_id: 0,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&window_size.to_be_bytes()),
            },
            RtmpMessage::SetPeerBandwidth {
                bandwidth,
                limit_type,
            } => RawMessage {
                msg_type: MessageType::SetPeerBandwidth,
                stream_id: 0,
                timestamp: 0,
                payload: Bytes::from([&bandwidth.to_be_bytes()[..], &[limit_type]].concat()),
            },
            RtmpMessage::CommandMessageAmf3 { values, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf3,
                stream_id,
                timestamp: 0,
                payload: encode_avmplus_values(&values)?,
            },
            RtmpMessage::CommandMessageAmf0 { values, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf0,
                stream_id,
                timestamp: 0,
                payload: encode_amf0_values(&values)?,
            },
            RtmpMessage::Event { event, stream_id } => event_into_raw(event, stream_id)?,
            RtmpMessage::SharedObjectAmf0 {
                name,
                version,
                persistent,
                events,
            } => serialize_shared_object(name, version, persistent, events, false),

            RtmpMessage::SharedObjectAmf3 {
                name,
                version,
                persistent,
                events,
            } => serialize_shared_object(name, version, persistent, events, true),
            RtmpMessage::AggregateMessage {
                stream_id,
                messages,
            } => serialize_aggregate_message(stream_id, messages)?,
        };
        Ok(result)
    }
}

/// Serialize a User Control message (type 4).
/// Layout: 2-byte event type | event-specific payload.
fn serialize_user_control(event: UserControlEvent) -> RawMessage {
    let (event_type_u16, data): (u16, Vec<u8>) = match event {
        UserControlEvent::StreamBegin { stream_id } => (0, stream_id.to_be_bytes().into()),
        UserControlEvent::StreamEof { stream_id } => (1, stream_id.to_be_bytes().into()),
        UserControlEvent::StreamDry { stream_id } => (2, stream_id.to_be_bytes().into()),
        UserControlEvent::SetBufferLength {
            stream_id,
            buffer_length_ms,
        } => {
            let mut d = stream_id.to_be_bytes().to_vec();
            d.extend_from_slice(&buffer_length_ms.to_be_bytes());
            (3, d)
        }
        UserControlEvent::StreamIsRecorded { stream_id } => (4, stream_id.to_be_bytes().into()),
        UserControlEvent::PingRequest { timestamp } => (6, timestamp.to_be_bytes().into()),
        UserControlEvent::PingResponse { timestamp } => (7, timestamp.to_be_bytes().into()),
        UserControlEvent::Unknown { event_type, data } => (event_type, data.to_vec()),
    };

    let payload = [&event_type_u16.to_be_bytes()[..], data.as_slice()].concat();
    RawMessage {
        msg_type: MessageType::UserControl,
        stream_id: 0,
        timestamp: 0,
        payload: Bytes::from(payload),
    }
}

/// Serialize a Shared Object message (AMF0 = type 19, AMF3 = type 16).
///
/// Wire format:
///   2 bytes  – name length
///   N bytes  – name (UTF-8)
///   4 bytes  – version
///   4 bytes  – flags  (bit 0 = persistent)
///   For each event:
///     1 byte  – event type
///     4 bytes – event data length
///     N bytes – event data
fn serialize_shared_object(
    name: String,
    version: u32,
    persistent: bool,
    events: Vec<crate::message::SharedObjectEvent>,
    amf3: bool,
) -> RawMessage {
    let name_bytes = name.as_bytes();
    let flags: u32 = if persistent { 1 } else { 0 };

    let capacity =
        2 + name_bytes.len() + 4 + 4 + events.iter().map(|e| 1 + 4 + e.data.len()).sum::<usize>();
    let mut buf = BytesMut::with_capacity(capacity);

    buf.put_u16(name_bytes.len() as u16);
    buf.put_slice(name_bytes);
    buf.put_u32(version);
    buf.put_u32(flags);

    for event in events {
        buf.put_u8(event.event_type);
        buf.put_u32(event.data.len() as u32);
        buf.put_slice(&event.data);
    }

    RawMessage {
        msg_type: if amf3 {
            MessageType::SharedObjectAmf3
        } else {
            MessageType::SharedObjectAmf0
        },
        stream_id: 0,
        timestamp: 0,
        payload: buf.freeze(),
    }
}

/// Serialize an Aggregate message (type 22).
///
/// Each sub-message is written in FLV tag format:
///   1 byte  – message type ID
///   3 bytes – payload length (big-endian)
///   3 bytes – timestamp low 24 bits (big-endian)
///   1 byte  – timestamp high 8 bits
///   3 bytes – stream ID (written as 0; inherited from aggregate header)
///   N bytes – payload
///   4 bytes – back pointer (header size 11 + payload length)
fn serialize_aggregate_message(
    stream_id: u32,
    messages: Vec<RtmpMessage>,
) -> Result<RawMessage, SerializationError> {
    let mut buf = BytesMut::new();
    let mut first_timestamp: Option<u32> = None;

    for msg in messages {
        let raw = msg.into_raw()?;
        let ts = raw.timestamp;
        first_timestamp.get_or_insert(ts);

        let data_size = raw.payload.len() as u32;
        let back_pointer: u32 = 11 + data_size;

        // type ID
        buf.put_u8(raw.msg_type.into_raw());
        // 3-byte payload length
        buf.put_u8(((data_size >> 16) & 0xFF) as u8);
        buf.put_u8(((data_size >> 8) & 0xFF) as u8);
        buf.put_u8((data_size & 0xFF) as u8);
        // 4-byte timestamp: low 3 bytes then high byte
        buf.put_u8(((ts >> 16) & 0xFF) as u8);
        buf.put_u8(((ts >> 8) & 0xFF) as u8);
        buf.put_u8((ts & 0xFF) as u8);
        buf.put_u8(((ts >> 24) & 0xFF) as u8);
        // 3-byte stream ID (always 0 in the FLV body; real stream_id is in chunk header)
        buf.put_u8(0);
        buf.put_u8(0);
        buf.put_u8(0);
        // payload
        buf.put_slice(&raw.payload);
        // back pointer
        buf.put_u32(back_pointer);
    }

    Ok(RawMessage {
        msg_type: MessageType::AggregateMessage,
        stream_id,
        timestamp: first_timestamp.unwrap_or(0),
        payload: buf.freeze(),
    })
}
