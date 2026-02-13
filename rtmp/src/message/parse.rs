use crate::{
    ParseError, RtmpEvent, ScriptData,
    message::{
        RtmpMessage,
        event::{audio_event_from_raw, video_event_from_raw},
    },
    protocol::{MessageType, RawMessage},
};

impl RtmpMessage {
    pub fn from_raw(msg: RawMessage) -> Result<Self, ParseError> {
        let result = match msg.msg_type {
            MessageType::SetChunkSize if msg.payload.len() >= 4 => {
                let p = &msg.payload;
                let chunk_size = u32::from_be_bytes([p[0] & 0x7F, p[1], p[2], p[3]]);
                // TODO: double check p[0] or p[3]
                RtmpMessage::SetChunkSize { chunk_size }
            }
            MessageType::SetChunkSize => return Err(ParseError::NotEnoughData),
            MessageType::CommandMessageAmf0 => todo!(),
            MessageType::AbortMessage => todo!(),
            MessageType::UserControl => todo!(),
            MessageType::SetPeerBandwidth => todo!(),
            MessageType::Audio => audio_event_from_raw(msg)?,
            MessageType::Video => video_event_from_raw(msg)?,
            MessageType::DataMessageAmf0 => RtmpMessage::Event {
                event: RtmpEvent::Metadata(ScriptData::parse(msg.payload)?),
                stream_id: msg.stream_id,
            },
            MessageType::Acknowledgement => todo!(),
            MessageType::WindowAckSize => todo!(),
        };
        Ok(result)
    }
}
