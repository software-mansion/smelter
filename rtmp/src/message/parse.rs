use bytes::Buf;

use crate::{
    AmfDecodingError, RtmpEvent, RtmpMessageParseError, ScriptData,
    amf0::decode_amf0_values,
    message::{
        RtmpMessage,
        event::{audio_event_from_raw, video_event_from_raw},
    },
    protocol::{MessageType, RawMessage, UserControlMessageKind},
};

impl RtmpMessage {
    pub fn from_raw(mut msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        let p = &msg.payload;
        let msg_type = MessageType::try_from_raw(msg.msg_type)?;
        let result = match msg_type {
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
            MessageType::SetChunkSize => {
                return Err(RtmpMessageParseError::PayloadToShort);
            }

            MessageType::WindowAckSize if msg.payload.len() >= 4 => RtmpMessage::WindowAckSize {
                window_size: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
            },
            MessageType::WindowAckSize => {
                return Err(RtmpMessageParseError::PayloadToShort);
            }

            MessageType::SetPeerBandwidth if msg.payload.len() >= 5 => {
                RtmpMessage::SetPeerBandwidth {
                    bandwidth: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
                    limit_type: p[4],
                }
            }
            MessageType::SetPeerBandwidth => {
                return Err(RtmpMessageParseError::PayloadToShort);
            }

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
                return Err(RtmpMessageParseError::UnsupportedMessage(format!(
                    "{msg_type:?}",
                )));
            }
            MessageType::AbortMessage => {
                return Err(RtmpMessageParseError::UnsupportedMessage(format!(
                    "{msg_type:?}",
                )));
            }
            MessageType::UserControl => {
                if p.len() < 2 {
                    return Err(RtmpMessageParseError::PayloadToShort);
                }
                let kind = UserControlMessageKind::from_raw(u16::from_be_bytes([p[0], p[1]]))?;
                match kind {
                    UserControlMessageKind::StreamBegin if p.len() >= 6 => {
                        let stream_id = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                        Self::StreamBegin { stream_id }
                    }
                    UserControlMessageKind::StreamBegin => {
                        return Err(RtmpMessageParseError::PayloadToShort);
                    }
                    kind => {
                        return Err(RtmpMessageParseError::UnsupportedMessage(format!(
                            "{kind:?}",
                        )));
                    }
                }
            }
        };
        Ok(result)
    }
}
