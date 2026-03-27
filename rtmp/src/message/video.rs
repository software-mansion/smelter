use std::time::Duration;

use crate::{
    FlvVideoData, GenericVideoData, H264VideoConfig, H264VideoData, RtmpMessageParseError,
    RtmpMessageSerializeError, VideoCodec, VideoTag, VideoTagFrameType, VideoTagH264PacketType,
    error::FlvVideoTagParseError,
    message::VIDEO_CHUNK_STREAM_ID,
    protocol::{MessageType, RawMessage},
};

#[derive(Debug, Clone)]
pub(crate) enum VideoMessage {
    H264Data(H264VideoData),
    H264Config(H264VideoConfig),
    Unknown(GenericVideoData), // TODO: consider completely replace Generic with Enhanced
}

impl VideoMessage {
    pub(super) fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        let tag = match FlvVideoData::parse(msg.payload)? {
            FlvVideoData::Legacy(tag) => tag,
            FlvVideoData::Enhanced(_) => unimplemented!(),
        };
        let event: VideoMessage = match (tag.codec, tag.h264_packet_type) {
            (VideoCodec::H264, Some(VideoTagH264PacketType::Data)) => {
                Self::H264Data(H264VideoData {
                    pts: Duration::from_millis(
                        (msg.timestamp as i64 + tag.composition_time.unwrap_or(0) as i64) as u64,
                    ),
                    dts: Duration::from_millis(msg.timestamp.into()),
                    data: tag.data,
                    is_keyframe: match tag.frame_type {
                        VideoTagFrameType::Keyframe => true,
                        VideoTagFrameType::Interframe => false,
                        _ => {
                            return Err(FlvVideoTagParseError::InvalidFrameTypeForH264(
                                tag.frame_type,
                            )
                            .into());
                        }
                    },
                })
            }
            (VideoCodec::H264, Some(VideoTagH264PacketType::Config)) => {
                Self::H264Config(H264VideoConfig { data: tag.data })
            }
            // TODO
            // (VideoCodec::H264, Some(VideoTagH264PacketType::Eos)) => {

            // }
            (codec, _) => Self::Unknown(GenericVideoData {
                timestamp: msg.timestamp,
                codec,
                data: tag.data,
                frame_type: tag.frame_type,
            }),
        };
        Ok(event)
    }

    pub(super) fn into_raw(self, stream_id: u32) -> Result<RawMessage, RtmpMessageSerializeError> {
        let result = match self {
            Self::H264Data(chunk) => RawMessage {
                msg_type: MessageType::Video.into_raw(),
                stream_id,
                chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
                timestamp: chunk.dts.as_millis() as u32,
                payload: FlvVideoData::Legacy(VideoTag {
                    h264_packet_type: Some(VideoTagH264PacketType::Data),
                    codec: VideoCodec::H264,
                    composition_time: Some(
                        (chunk.pts.as_millis() as i64 - chunk.dts.as_millis() as i64) as i32,
                    ),
                    frame_type: match chunk.is_keyframe {
                        true => VideoTagFrameType::Keyframe,
                        false => VideoTagFrameType::Interframe,
                    },
                    data: chunk.data,
                })
                .serialize()?,
            },
            Self::H264Config(config) => RawMessage {
                msg_type: MessageType::Video.into_raw(),
                stream_id,
                chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: FlvVideoData::Legacy(VideoTag {
                    h264_packet_type: Some(VideoTagH264PacketType::Config),
                    codec: VideoCodec::H264,
                    composition_time: Some(0),
                    frame_type: VideoTagFrameType::Keyframe,
                    data: config.data,
                })
                .serialize()?,
            },
            Self::Unknown(data) => RawMessage {
                msg_type: MessageType::Video.into_raw(),
                stream_id,
                chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
                timestamp: data.timestamp,
                payload: FlvVideoData::Legacy(VideoTag {
                    h264_packet_type: None,
                    codec: data.codec,
                    composition_time: None,
                    frame_type: data.frame_type,
                    data: data.data,
                })
                .serialize()?,
            },
        };
        Ok(result)
    }
}
