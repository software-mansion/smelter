use bytes::Buf;

use crate::{
    AmfDecodingError, ParseError, RtmpEvent, ScriptData,
    amf0::decode_amf0_values,
    message::{
        RtmpMessage,
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
                // TODO: double check p[0] or p[3]
                RtmpMessage::SetChunkSize { chunk_size }
            }
            MessageType::SetChunkSize => return Err(ParseError::NotEnoughData),

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

            MessageType::Acknowledgement => {
                return Err(ParseError::UnsupportedMessageType(msg.msg_type));
            }
            MessageType::AbortMessage => {
                return Err(ParseError::UnsupportedMessageType(msg.msg_type));
            }
            MessageType::UserControl => {
                return Err(ParseError::UnsupportedMessageType(msg.msg_type));
            }
        };
        Ok(result)
    }
}
