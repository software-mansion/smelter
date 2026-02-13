use crate::{
    AudioTag, ParseError, ScriptData, VideoTag,
    message::RtmpMessage,
    protocol::{MessageType, RawMessage},
};

impl RtmpMessage {
    pub fn from_raw(msg: RawMessage) -> Result<Self, ParseError> {
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
