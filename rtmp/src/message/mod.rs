use bytes::Bytes;

use crate::{
    AudioTag, ParseError, ScriptData, SerializationError, VideoTag,
    amf0::{Amf0Value, encode_amf_values},
    protocol::{MessageType, RawMessage, UserControlMessageEvent},
};

#[derive(Debug)]
pub enum RtmpMessage {
    WindowAckSize {
        window_size: u32,
    },
    SetPeerBandwidth {
        bandwidth: u32,
        limit_type: u8,
    },
    StreamBegin {
        stream_id: u32,
    },
    CommandMessageAmf0 {
        values: Vec<Amf0Value>,
        stream_id: u32,
    },
    SetChunkSize {
        chunk_size: u32,
    },
    Audio {
        tag: AudioTag,
        timestamp: i64,
        stream_id: u32,
    },
    Video {
        tag: VideoTag,
        timestamp: i64,
        stream_id: u32,
    },
    ScriptData(ScriptData),
}

impl TryFrom<RtmpMessage> for RawMessage {
    type Error = SerializationError;

    fn try_from(msg: RtmpMessage) -> Result<Self, Self::Error> {
        let result = match msg {
            RtmpMessage::WindowAckSize { window_size } => RawMessage {
                msg_type: MessageType::WindowAckSize,
                stream_id: 0,
                timestamp: 0,
                payload: Bytes::copy_from_slice(&window_size.to_be_bytes()[..]),
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
            RtmpMessage::StreamBegin { stream_id } => RawMessage {
                msg_type: MessageType::UserControl,
                stream_id: 0,
                timestamp: 0,
                payload: Bytes::from(
                    [
                        &[6u8][..],
                        &stream_id.to_be_bytes(),
                        &(UserControlMessageEvent::StreamBegin as u16).to_be_bytes(),
                    ]
                    .concat(),
                ),
            },
            RtmpMessage::CommandMessageAmf0 { values, stream_id } => RawMessage {
                msg_type: MessageType::CommandMessageAmf0,
                stream_id,
                timestamp: 0,
                payload: encode_amf_values(&values)?,
            },
            RtmpMessage::SetChunkSize { chunk_size } => RawMessage {
                msg_type: MessageType::SetChunkSize,
                stream_id: 0, // TODO: not sure if zero
                timestamp: 0,
                payload: Bytes::copy_from_slice(&chunk_size.to_be_bytes()[..]),
            },
            RtmpMessage::Audio {
                tag,
                timestamp,
                stream_id,
            } => RawMessage {
                msg_type: MessageType::Audio,
                stream_id,
                timestamp: timestamp as u32,
                payload: tag.serialize()?,
            },
            RtmpMessage::Video {
                tag,
                timestamp,
                stream_id,
            } => RawMessage {
                msg_type: MessageType::Video,
                stream_id,
                timestamp: timestamp as u32,
                payload: tag.serialize()?,
            },
            RtmpMessage::ScriptData(script_data) => RawMessage {
                msg_type: MessageType::DataMessageAmf0,
                stream_id: 0,
                timestamp: 0,
                payload: script_data.serialize()?,
            },
        };
        Ok(result)
    }
}

impl TryFrom<RawMessage> for RtmpMessage {
    type Error = ParseError;

    fn try_from(msg: RawMessage) -> Result<Self, Self::Error> {
        match msg.msg_type {
            MessageType::SetChunkSize if msg.payload.len() >= 4 => {
                let p = &msg.payload;
                let chunk_size = u32::from_be_bytes([p[0] & 0x7F, p[1], p[2], p[3]]);
                // TODO: double check p[0] or p[3]
                Ok(RtmpMessage::SetChunkSize { chunk_size })
            }
            MessageType::SetChunkSize => Err(ParseError::NotEnoughData),
            MessageType::CommandMessageAmf0 => todo!(),
            MessageType::AbortMessage => todo!(),
            MessageType::UserControl => todo!(),
            MessageType::SetPeerBandwidth => todo!(),
            MessageType::Audio => Ok(RtmpMessage::Audio {
                tag: AudioTag::parse(msg.payload)?,
                timestamp: msg.timestamp as i64,
                stream_id: msg.stream_id,
            }),
            MessageType::Video => Ok(RtmpMessage::Video {
                tag: VideoTag::parse(msg.payload)?,
                timestamp: msg.timestamp as i64,
                stream_id: msg.stream_id,
            }),
            MessageType::DataMessageAmf0 => {
                Ok(RtmpMessage::ScriptData(ScriptData::parse(msg.payload)?))
            }
            MessageType::Acknowledgement => todo!(),
            MessageType::WindowAckSize => todo!(),
        }
    }
}
