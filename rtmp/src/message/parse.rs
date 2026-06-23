use crate::{
    RtmpMessageParseError,
    amf0::decode_amf_values,
    message::{
        DataMessage, RtmpMessageIncoming, audio::AudioMessage, command::CommandMessage,
        user_control::UserControlMessage, video::VideoMessage,
    },
    protocol::{MessageType, RawMessage},
};

impl RtmpMessageIncoming {
    pub fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        let p = &msg.payload;
        let msg_type = MessageType::try_from_raw(msg.msg_type)?;
        let result = match msg_type {
            MessageType::Audio => {
                RtmpMessageIncoming::Audio { audio: AudioMessage::from_raw(msg)? }
            }
            MessageType::Video => {
                RtmpMessageIncoming::Video { video: VideoMessage::from_raw(msg)? }
            }

            MessageType::DataMessageAmf0 => RtmpMessageIncoming::DataMessage {
                data: DataMessage::from_amf_values(decode_amf_values(msg.payload)?),
            },

            MessageType::SetChunkSize if msg.payload.len() >= 4 => {
                // top bit is reserved (0), low 31 bits are the size (RTMP 5.4.1).
                let chunk_size = u32::from_be_bytes([p[0] & 0x7F, p[1], p[2], p[3]]);
                RtmpMessageIncoming::SetChunkSize { chunk_size }
            }
            MessageType::SetChunkSize => {
                return Err(RtmpMessageParseError::PayloadTooShort);
            }

            MessageType::WindowAckSize if msg.payload.len() >= 4 => {
                RtmpMessageIncoming::WindowAckSize {
                    window_size: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
                }
            }
            MessageType::WindowAckSize => {
                return Err(RtmpMessageParseError::PayloadTooShort);
            }

            MessageType::SetPeerBandwidth if msg.payload.len() >= 5 => {
                RtmpMessageIncoming::SetPeerBandwidth {
                    bandwidth: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
                    limit_type: p[4],
                }
            }
            MessageType::SetPeerBandwidth => {
                return Err(RtmpMessageParseError::PayloadTooShort);
            }

            MessageType::CommandMessageAmf0 => RtmpMessageIncoming::CommandMessage {
                msg: CommandMessage::from_amf0_bytes(msg.payload)?,
                stream_id: msg.stream_id,
            },

            MessageType::Acknowledgement if p.len() >= 4 => {
                RtmpMessageIncoming::Acknowledgement {
                    bytes_received: u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
                }
            }
            MessageType::Acknowledgement => {
                return Err(RtmpMessageParseError::PayloadTooShort);
            }

            MessageType::AbortMessage => {
                return Err(RtmpMessageParseError::UnsupportedMessage(format!(
                    "{msg_type:?}",
                )));
            }
            MessageType::UserControl => {
                RtmpMessageIncoming::UserControl(UserControlMessage::from_raw(p)?)
            }
        };
        Ok(result)
    }
}
