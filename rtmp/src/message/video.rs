use std::time::Duration;

use crate::{
    EnhancedVideoData, ExVideoFourCc, ExVideoPacket, ExVideoTag, FlvVideoData, H264VideoConfig,
    H264VideoData, LegacyVideoData, RtmpMessageParseError, RtmpMessageSerializeError, VideoCodec,
    VideoTag, VideoTagFrameType, VideoTagH264PacketType,
    error::FlvVideoTagParseError,
    message::VIDEO_CHUNK_STREAM_ID,
    protocol::{MessageType, RawMessage},
};

#[derive(Debug, Clone)]
pub(crate) enum VideoMessage {
    H264Data(H264VideoData),
    H264Config(H264VideoConfig),
    Legacy(LegacyVideoData),
    Enhanced(EnhancedVideoData),
}

impl VideoMessage {
    pub(crate) fn is_media_packet(&self) -> bool {
        match self {
            Self::H264Config(_) => false,
            Self::Enhanced(data) => matches!(
                data.tag,
                ExVideoTag::VideoBody {
                    packet: ExVideoPacket::CodedFrames { .. },
                    ..
                }
            ),
            _ => true,
        }
    }

    pub(super) fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        match FlvVideoData::parse(msg.payload)? {
            FlvVideoData::Legacy(tag) => Self::from_legacy(msg.timestamp, tag),
            FlvVideoData::Enhanced(tag) => Self::from_enhanced(msg.timestamp, tag),
        }
    }

    fn from_legacy(timestamp: u32, tag: VideoTag) -> Result<Self, RtmpMessageParseError> {
        let event = match (tag.codec, tag.h264_packet_type) {
            (VideoCodec::H264, Some(VideoTagH264PacketType::Data)) => {
                Self::H264Data(H264VideoData {
                    pts: Duration::from_millis(
                        (timestamp as i64 + tag.composition_time.unwrap_or(0) as i64) as u64,
                    ),
                    dts: Duration::from_millis(timestamp.into()),
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
            (_, _) => Self::Legacy(LegacyVideoData { timestamp, tag }),
        };
        Ok(event)
    }

    fn from_enhanced(timestamp: u32, tag: ExVideoTag) -> Result<Self, RtmpMessageParseError> {
        if let ExVideoTag::VideoBody {
            four_cc: ExVideoFourCc::Avc1,
            packet: ExVideoPacket::SequenceStart(data),
            ..
        } = &tag
        {
            return Ok(Self::H264Config(H264VideoConfig { data: data.clone() }));
        }

        if let ExVideoTag::VideoBody {
            four_cc: ExVideoFourCc::Avc1,
            packet:
                ExVideoPacket::CodedFrames {
                    composition_time,
                    data,
                },
            frame_type,
            ..
        } = &tag
        {
            let is_keyframe = match frame_type {
                VideoTagFrameType::Keyframe => true,
                VideoTagFrameType::Interframe => false,
                _ => {
                    return Err(FlvVideoTagParseError::InvalidFrameTypeForH264(*frame_type).into());
                }
            };

            return Ok(Self::H264Data(H264VideoData {
                pts: Duration::from_millis((timestamp as i64 + *composition_time as i64) as u64),
                dts: Duration::from_millis(timestamp.into()),
                data: data.clone(),
                is_keyframe,
            }));
        }

        Ok(Self::Enhanced(EnhancedVideoData { timestamp, tag }))
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
            Self::Legacy(data) => RawMessage {
                msg_type: MessageType::Video.into_raw(),
                stream_id,
                chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
                timestamp: data.timestamp,
                payload: FlvVideoData::Legacy(data.tag).serialize()?,
            },
            Self::Enhanced(data) => RawMessage {
                msg_type: MessageType::Video.into_raw(),
                stream_id,
                chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
                timestamp: data.timestamp,
                payload: FlvVideoData::Enhanced(data.tag).serialize()?,
            },
        };
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::VideoMessage;
    use crate::{
        EnhancedVideoData, ExVideoFourCc, ExVideoPacket, ExVideoTag, FlvVideoData,
        VideoTagFrameType,
        protocol::{MessageType, RawMessage},
    };

    #[test]
    fn parses_enhanced_avc1_coded_frames_as_h264_data() {
        let payload = FlvVideoData::Enhanced(ExVideoTag::VideoBody {
            four_cc: ExVideoFourCc::Avc1,
            packet: ExVideoPacket::CodedFrames {
                composition_time: 5,
                data: Bytes::from_static(b"frame"),
            },
            frame_type: VideoTagFrameType::Keyframe,
            timestamp_offset_nanos: None,
        })
        .serialize()
        .unwrap();

        let message = VideoMessage::from_raw(RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id: 1,
            chunk_stream_id: 6,
            timestamp: 100,
            payload,
        })
        .unwrap();

        match message {
            VideoMessage::H264Data(data) => {
                assert_eq!(data.dts.as_millis() as u32, 100);
                assert_eq!(data.pts.as_millis() as u32, 105);
                assert!(data.is_keyframe);
                assert_eq!(data.data, Bytes::from_static(b"frame"));
            }
            other => panic!("expected H264Data, got {other:?}"),
        }
    }

    #[test]
    fn parses_non_avc_enhanced_as_enhanced_variant() {
        let original_tag = ExVideoTag::VideoBody {
            four_cc: ExVideoFourCc::Vp09,
            packet: ExVideoPacket::CodedFrames {
                composition_time: 0,
                data: Bytes::from_static(b"vp9"),
            },
            frame_type: VideoTagFrameType::Interframe,
            timestamp_offset_nanos: Some(777),
        };
        let payload = FlvVideoData::Enhanced(original_tag.clone())
            .serialize()
            .unwrap();

        let message = VideoMessage::from_raw(RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id: 1,
            chunk_stream_id: 6,
            timestamp: 33,
            payload,
        })
        .unwrap();

        match message {
            VideoMessage::Enhanced(data) => {
                assert_eq!(data.timestamp, 33);
                assert_eq!(data.tag, original_tag);
            }
            other => panic!("expected Enhanced, got {other:?}"),
        }
    }

    #[test]
    fn serializes_and_reparses_enhanced_video_message() {
        let tag = ExVideoTag::StartSeek;
        let raw = VideoMessage::Enhanced(EnhancedVideoData {
            timestamp: 42,
            tag: tag.clone(),
        })
        .into_raw(10)
        .unwrap();

        assert_eq!(raw.msg_type, MessageType::Video.into_raw());
        assert_eq!(raw.stream_id, 10);
        assert_eq!(raw.timestamp, 42);

        let reparsed = FlvVideoData::parse(raw.payload).unwrap();
        match reparsed {
            FlvVideoData::Enhanced(parsed_tag) => assert_eq!(parsed_tag, tag),
            other => panic!("expected Enhanced flv payload, got {other:?}"),
        }
    }
}
