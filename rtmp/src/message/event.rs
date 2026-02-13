use crate::{
    RtmpEvent, SerializationError,
    message::RtmpMessage,
    protocol::{MessageType, RawMessage},
};

pub(super) fn event_from_raw(msg: RawMessage) -> Result<RtmpMessage, ParseError> {}

pub(super) fn event_into_raw(
    event: RtmpEvent,
    stream_id: u32,
) -> Result<RawMessage, SerializationError> {
    let result = match event {
        RtmpEvent::H264Data(data) => RawMessage {
            msg_type: MessageType::Video,
            stream_id,
            timestamp: todo!(),
            payload: todo!(),
        },
        RtmpEvent::H264Config(config) => todo!(),
        RtmpEvent::AacData(data) => todo!(),
        RtmpEvent::AacConfig(config) => todo!(),
        RtmpEvent::GenericAudioData(data) => todo!(),
        RtmpEvent::GenericVideoData(data) => todo!(),
        RtmpEvent::Metadata(script_data) => todo!(),
    };
    (result)
}
