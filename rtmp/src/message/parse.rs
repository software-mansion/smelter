use bytes::{Buf, Bytes};

use crate::{
    AmfDecodingError, ParseError, RtmpEvent, ScriptData,
    amf0::decode_amf0_values,
    message::{
        RtmpMessage, SharedObjectEvent, UserControlEvent,
        event::{audio_event_from_raw, video_event_from_raw},
    },
    protocol::{MessageType, RawMessage},
};

impl RtmpMessage {
    pub fn from_raw(mut msg: RawMessage) -> Result<Self, ParseError> {
        let p = &msg.payload;
        let result = match msg.msg_type {
            MessageType::Audio => audio_event_from_raw(msg)?,
            MessageType::Video => video_event_from_raw(msg)?,

            MessageType::DataMessageAmf3 => {
                let format_selector = msg.payload.get_u8();
                if format_selector != 0 {
                    return Err(AmfDecodingError::InvalidFormatSelector.into());
                }

                RtmpMessage::Event {
                    event: RtmpEvent::Metadata(ScriptData::parse(msg.payload)?),
                    stream_id: msg.stream_id,
                }
            }
            MessageType::DataMessageAmf0 => RtmpMessage::Event {
                event: RtmpEvent::Metadata(ScriptData::parse(msg.payload)?),
                stream_id: msg.stream_id,
            },

            MessageType::SetChunkSize if msg.payload.len() >= 4 => {
                let chunk_size = u32::from_be_bytes([p[0] & 0x7F, p[1], p[2], p[3]]);
                RtmpMessage::SetChunkSize { chunk_size }
            }
            MessageType::SetChunkSize => return Err(ParseError::NotEnoughData),

            MessageType::AbortMessage if msg.payload.len() >= 4 => RtmpMessage::AbortMessage {
                chunk_stream_id: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
            },
            MessageType::AbortMessage => return Err(ParseError::NotEnoughData),

            MessageType::Acknowledgement if msg.payload.len() >= 4 => {
                RtmpMessage::Acknowledgement {
                    sequence_number: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
                }
            }
            MessageType::Acknowledgement => return Err(ParseError::NotEnoughData),

            MessageType::UserControl => parse_user_control(msg.payload)?,

            MessageType::WindowAckSize if msg.payload.len() >= 4 => RtmpMessage::WindowAckSize {
                window_size: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
            },
            MessageType::WindowAckSize => return Err(ParseError::NotEnoughData),

            MessageType::SetPeerBandwidth if msg.payload.len() >= 5 => {
                RtmpMessage::SetPeerBandwidth {
                    bandwidth: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
                    limit_type: p[4],
                }
            }
            MessageType::SetPeerBandwidth => return Err(ParseError::NotEnoughData),

            MessageType::CommandMessageAmf3 => {
                let format_selector = msg.payload.get_u8();
                if format_selector != 0 {
                    return Err(AmfDecodingError::InvalidFormatSelector.into());
                }
                RtmpMessage::CommandMessageAmf3 {
                    values: decode_amf0_values(msg.payload)?,
                    stream_id: msg.stream_id,
                }
            }
            MessageType::CommandMessageAmf0 => RtmpMessage::CommandMessageAmf0 {
                values: decode_amf0_values(msg.payload)?,
                stream_id: msg.stream_id,
            },

            // SharedObjectAmf3 does not have a format-selector prefix; its event data
            // bytes are AMF3 encoded (callers decode as needed).
            MessageType::SharedObjectAmf3 => parse_shared_object(msg.payload, true)?,
            MessageType::SharedObjectAmf0 => parse_shared_object(msg.payload, false)?,

            MessageType::AggregateMessage => parse_aggregate_message(msg.stream_id, msg.payload)?,
        };
        Ok(result)
    }
}

/// Parse a User Control message (type 4).
fn parse_user_control(mut payload: Bytes) -> Result<RtmpMessage, ParseError> {
    if payload.remaining() < 2 {
        return Err(ParseError::NotEnoughData);
    }
    let event_type = payload.get_u16();

    let event = match event_type {
        // StreamBegin – stream_id (4 bytes)
        0 => {
            if payload.remaining() < 4 {
                return Err(ParseError::NotEnoughData);
            }
            UserControlEvent::StreamBegin {
                stream_id: payload.get_u32(),
            }
        }
        // StreamEof – stream_id (4 bytes)
        1 => {
            if payload.remaining() < 4 {
                return Err(ParseError::NotEnoughData);
            }
            UserControlEvent::StreamEof {
                stream_id: payload.get_u32(),
            }
        }
        // StreamDry – stream_id (4 bytes)
        2 => {
            if payload.remaining() < 4 {
                return Err(ParseError::NotEnoughData);
            }
            UserControlEvent::StreamDry {
                stream_id: payload.get_u32(),
            }
        }
        // SetBufferLength – stream_id (4 bytes) + buffer_length_ms (4 bytes)
        3 => {
            if payload.remaining() < 8 {
                return Err(ParseError::NotEnoughData);
            }
            let stream_id = payload.get_u32();
            let buffer_length_ms = payload.get_u32();
            UserControlEvent::SetBufferLength {
                stream_id,
                buffer_length_ms,
            }
        }
        // StreamIsRecorded – stream_id (4 bytes)
        4 => {
            if payload.remaining() < 4 {
                return Err(ParseError::NotEnoughData);
            }
            UserControlEvent::StreamIsRecorded {
                stream_id: payload.get_u32(),
            }
        }
        // PingRequest – timestamp (4 bytes)
        6 => {
            if payload.remaining() < 4 {
                return Err(ParseError::NotEnoughData);
            }
            UserControlEvent::PingRequest {
                timestamp: payload.get_u32(),
            }
        }
        // PingResponse – timestamp (4 bytes)
        7 => {
            if payload.remaining() < 4 {
                return Err(ParseError::NotEnoughData);
            }
            UserControlEvent::PingResponse {
                timestamp: payload.get_u32(),
            }
        }
        // Unknown event type – preserve raw bytes for forward compatibility
        _ => UserControlEvent::Unknown {
            event_type,
            data: payload,
        },
    };
    Ok(RtmpMessage::UserControl { event })
}

/// Parse a Shared Object message (types 19 = AMF0, 16 = AMF3).
///
/// Both variants share the same outer binary structure:
///   2 bytes  – object name length
///   N bytes  – object name (UTF-8)
///   4 bytes  – current version
///   4 bytes  – flags (bit 0 = persistent)
///   Repeated until end of payload:
///     1 byte  – event type
///     4 bytes – event data length
///     N bytes – event data (AMF0 or AMF3 encoded)
fn parse_shared_object(mut payload: Bytes, amf3: bool) -> Result<RtmpMessage, ParseError> {
    if payload.remaining() < 2 {
        return Err(ParseError::NotEnoughData);
    }
    let name_len = payload.get_u16() as usize;

    if payload.remaining() < name_len + 8 {
        return Err(ParseError::NotEnoughData);
    }
    let name_bytes = payload.copy_to_bytes(name_len);
    let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;

    let version = payload.get_u32();
    let flags = payload.get_u32();
    let persistent = flags & 1 != 0;

    let mut events = Vec::new();
    while payload.remaining() >= 5 {
        let event_type = payload.get_u8();
        let event_data_len = payload.get_u32() as usize;
        if payload.remaining() < event_data_len {
            break;
        }
        let data = payload.copy_to_bytes(event_data_len);
        events.push(SharedObjectEvent { event_type, data });
    }

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

/// Parse an Aggregate message (type 22).
///
/// The payload is a sequence of FLV-style tagged sub-messages:
///   1 byte  – message type ID
///   3 bytes – payload length (big-endian)
///   3 bytes – timestamp low 24 bits (big-endian)
///   1 byte  – timestamp high 8 bits (extends to 32-bit timestamp)
///   3 bytes – stream ID (little-endian, per FLV spec; typically ignored)
///   N bytes – sub-message payload
///   4 bytes – back pointer (size of the entire preceding entry incl. header)
///
/// Sub-messages inherit the aggregate message's `stream_id`. Unknown or
/// unparseable sub-message types are silently skipped to maximise compatibility
/// with real-world streams that embed proprietary data in aggregates.
fn parse_aggregate_message(stream_id: u32, mut payload: Bytes) -> Result<RtmpMessage, ParseError> {
    let mut messages = Vec::new();

    while payload.remaining() >= 11 {
        let type_id = payload.get_u8();

        // 3-byte big-endian payload length
        let d0 = payload.get_u8() as u32;
        let d1 = payload.get_u8() as u32;
        let d2 = payload.get_u8() as u32;
        let data_size = (d0 << 16) | (d1 << 8) | d2;

        // 4-byte timestamp: 3 low bytes then 1 high byte
        let t0 = payload.get_u8() as u32;
        let t1 = payload.get_u8() as u32;
        let t2 = payload.get_u8() as u32;
        let t_ext = payload.get_u8() as u32;
        let timestamp = (t_ext << 24) | (t0 << 16) | (t1 << 8) | t2;

        // 3-byte stream ID (little-endian) – we use the aggregate's stream_id instead.
        payload.advance(3);

        let data_size = data_size as usize;
        // Need payload bytes + 4-byte back pointer
        if payload.remaining() < data_size + 4 {
            break;
        }

        let sub_payload = payload.copy_to_bytes(data_size);
        let _back_pointer = payload.get_u32();

        let msg_type = match MessageType::try_from_raw(type_id) {
            Ok(t) => t,
            // Skip sub-messages with unrecognised type IDs.
            Err(_) => continue,
        };

        let raw = RawMessage {
            msg_type,
            stream_id,
            timestamp,
            payload: sub_payload,
        };

        match RtmpMessage::from_raw(raw) {
            Ok(m) => messages.push(m),
            // Skip sub-messages that fail to parse (e.g. proprietary extensions).
            Err(_) => continue,
        }
    }

    Ok(RtmpMessage::AggregateMessage {
        stream_id,
        messages,
    })
}
