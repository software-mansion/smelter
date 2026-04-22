use std::time::Duration;

use crate::{
    ExVideoFourCc, ExVideoPacket, ExVideoTag, FlvVideoData, LegacyFlvVideoCodec,
    RtmpMessageParseError, RtmpMessageSerializeError, RtmpVideoCodec, TrackId, Video, VideoConfig,
    VideoTag, VideoTagFrameType, VideoTagH264PacketType,
    error::FlvVideoTagParseError,
    message::VIDEO_CHUNK_STREAM_ID,
    protocol::{MessageType, RawMessage},
};

#[derive(Debug, Clone)]
pub(crate) enum VideoMessage {
    Data(Video),
    Config(VideoConfig),
    /// Wire-level video packet types that carry no user-visible payload
    /// (seek commands, SequenceEnd, video-level metadata, MPEG2-TS sequence start).
    Unknown,
}

impl VideoMessage {
    pub(crate) fn is_media_packet(&self) -> bool {
        matches!(self, Self::Data(_))
    }

    /// Parses an incoming RTMP video message.
    pub(super) fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        match FlvVideoData::parse(msg.payload)? {
            FlvVideoData::Legacy(tag) => Self::from_legacy(msg.timestamp, tag),
            FlvVideoData::Enhanced(tag) => Ok(Self::from_enhanced(msg.timestamp, tag)?),
        }
    }

    fn from_legacy(timestamp: u32, tag: VideoTag) -> Result<Self, RtmpMessageParseError> {
        match (tag.codec, tag.h264_packet_type) {
            (LegacyFlvVideoCodec::H264, Some(VideoTagH264PacketType::Data)) => {
                let is_keyframe = match tag.frame_type {
                    VideoTagFrameType::Keyframe => true,
                    VideoTagFrameType::Interframe => false,
                    _ => {
                        return Err(
                            FlvVideoTagParseError::InvalidFrameTypeForH264(tag.frame_type).into(),
                        );
                    }
                };
                Ok(Self::Data(Video {
                    track_id: TrackId::PRIMARY,
                    codec: RtmpVideoCodec::H264,
                    pts: Duration::from_millis(
                        (timestamp as i64 + tag.composition_time.unwrap_or(0) as i64).max(0) as u64,
                    ),
                    dts: Duration::from_millis(timestamp.into()),
                    data: tag.data,
                    is_keyframe,
                }))
            }
            (LegacyFlvVideoCodec::H264, Some(VideoTagH264PacketType::Config)) => {
                Ok(Self::Config(VideoConfig {
                    track_id: TrackId::PRIMARY,
                    codec: RtmpVideoCodec::H264,
                    data: tag.data,
                }))
            }
            (codec, _) => {
                Err(FlvVideoTagParseError::UnsupportedLegacyVideoCodec(format!("{codec:?}")).into())
            }
        }
    }

    fn from_enhanced(timestamp: u32, tag: ExVideoTag) -> Result<Self, RtmpMessageParseError> {
        let (four_cc, packet, frame_type, timestamp_offset_nanos) = match tag {
            ExVideoTag::VideoBody {
                four_cc,
                packet,
                frame_type,
                timestamp_offset_nanos,
            } => (four_cc, packet, frame_type, timestamp_offset_nanos),
            ExVideoTag::StartSeek | ExVideoTag::EndSeek => return Ok(Self::Unknown),
        };

        let codec = video_codec_from_four_cc(four_cc);

        let nanos_offset = u64::from(timestamp_offset_nanos.unwrap_or(0));
        let dts = Duration::from_millis(timestamp.into()) + Duration::from_nanos(nanos_offset);

        match packet {
            ExVideoPacket::SequenceStart(data) => Ok(Self::Config(VideoConfig {
                track_id: TrackId::PRIMARY,
                codec,
                data,
            })),
            ExVideoPacket::CodedFrames {
                composition_time,
                data,
            } => {
                let is_keyframe = match frame_type {
                    VideoTagFrameType::Keyframe => true,
                    VideoTagFrameType::Interframe => false,
                    _ => {
                        return Err(
                            FlvVideoTagParseError::InvalidFrameTypeForH264(frame_type).into()
                        );
                    }
                };
                let pts = Duration::from_millis(
                    (timestamp as i64 + composition_time as i64).max(0) as u64,
                ) + Duration::from_nanos(nanos_offset);
                Ok(Self::Data(Video {
                    track_id: TrackId::PRIMARY,
                    codec,
                    pts,
                    dts,
                    data,
                    is_keyframe,
                }))
            }
            ExVideoPacket::SequenceEnd
            | ExVideoPacket::Metadata(_)
            | ExVideoPacket::Mpeg2TsSequenceStart(_) => Ok(Self::Unknown),
        }
    }

    pub(super) fn into_raw(self, stream_id: u32) -> Result<RawMessage, RtmpMessageSerializeError> {
        match self {
            Self::Data(video) => video_into_raw(video, stream_id),
            Self::Config(config) => config_into_raw(config, stream_id),
            Self::Unknown => Err(RtmpMessageSerializeError::InternalError(
                "Cannot serialize an unknown video message".into(),
            )),
        }
    }
}

fn video_codec_from_four_cc(four_cc: ExVideoFourCc) -> RtmpVideoCodec {
    match four_cc {
        ExVideoFourCc::Avc1 => RtmpVideoCodec::H264,
        ExVideoFourCc::Hvc1 => RtmpVideoCodec::Hevc,
        ExVideoFourCc::Vvc1 => RtmpVideoCodec::Vvc,
        ExVideoFourCc::Vp08 => RtmpVideoCodec::Vp8,
        ExVideoFourCc::Vp09 => RtmpVideoCodec::Vp9,
        ExVideoFourCc::Av01 => RtmpVideoCodec::Av1,
    }
}

fn four_cc_from_video_codec(codec: RtmpVideoCodec) -> ExVideoFourCc {
    match codec {
        RtmpVideoCodec::H264 => ExVideoFourCc::Avc1,
        RtmpVideoCodec::Hevc => ExVideoFourCc::Hvc1,
        RtmpVideoCodec::Vvc => ExVideoFourCc::Vvc1,
        RtmpVideoCodec::Vp8 => ExVideoFourCc::Vp08,
        RtmpVideoCodec::Vp9 => ExVideoFourCc::Vp09,
        RtmpVideoCodec::Av1 => ExVideoFourCc::Av01,
    }
}

fn video_into_raw(video: Video, stream_id: u32) -> Result<RawMessage, RtmpMessageSerializeError> {
    let timestamp = video.dts.as_millis() as u32;
    let payload = if video.codec == RtmpVideoCodec::H264 {
        FlvVideoData::Legacy(VideoTag {
            h264_packet_type: Some(VideoTagH264PacketType::Data),
            codec: LegacyFlvVideoCodec::H264,
            composition_time: Some(
                (video.pts.as_millis() as i64 - video.dts.as_millis() as i64) as i32,
            ),
            frame_type: match video.is_keyframe {
                true => VideoTagFrameType::Keyframe,
                false => VideoTagFrameType::Interframe,
            },
            data: video.data,
        })
        .serialize()?
    } else {
        let composition_time = (video.pts.as_millis() as i64 - video.dts.as_millis() as i64) as i32;
        FlvVideoData::Enhanced(ExVideoTag::VideoBody {
            four_cc: four_cc_from_video_codec(video.codec),
            packet: ExVideoPacket::CodedFrames {
                composition_time,
                data: video.data,
            },
            frame_type: match video.is_keyframe {
                true => VideoTagFrameType::Keyframe,
                false => VideoTagFrameType::Interframe,
            },
            timestamp_offset_nanos: None,
        })
        .serialize()?
    };

    Ok(RawMessage {
        msg_type: MessageType::Video.into_raw(),
        stream_id,
        chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
        timestamp,
        payload,
    })
}

fn config_into_raw(
    config: VideoConfig,
    stream_id: u32,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    let payload = if config.codec == RtmpVideoCodec::H264 {
        FlvVideoData::Legacy(VideoTag {
            h264_packet_type: Some(VideoTagH264PacketType::Config),
            codec: LegacyFlvVideoCodec::H264,
            composition_time: Some(0),
            frame_type: VideoTagFrameType::Keyframe,
            data: config.data,
        })
        .serialize()?
    } else {
        FlvVideoData::Enhanced(ExVideoTag::VideoBody {
            four_cc: four_cc_from_video_codec(config.codec),
            packet: ExVideoPacket::SequenceStart(config.data),
            frame_type: VideoTagFrameType::Keyframe,
            timestamp_offset_nanos: None,
        })
        .serialize()?
    };

    Ok(RawMessage {
        msg_type: MessageType::Video.into_raw(),
        stream_id,
        chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
        timestamp: 0,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::VideoMessage;
    use crate::{
        ExVideoFourCc, ExVideoPacket, ExVideoTag, FlvVideoData, RtmpVideoCodec, VideoTagFrameType,
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
            VideoMessage::Data(data) => {
                assert_eq!(data.codec, RtmpVideoCodec::H264);
                assert_eq!(data.dts.as_millis() as u32, 100);
                assert_eq!(data.pts.as_millis() as u32, 105);
                assert!(data.is_keyframe);
                assert_eq!(data.data, Bytes::from_static(b"frame"));
            }
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn parses_non_avc_enhanced_as_unified_video() {
        let payload = FlvVideoData::Enhanced(ExVideoTag::VideoBody {
            four_cc: ExVideoFourCc::Vp09,
            packet: ExVideoPacket::CodedFrames {
                composition_time: 0,
                data: Bytes::from_static(b"vp9"),
            },
            frame_type: VideoTagFrameType::Interframe,
            timestamp_offset_nanos: Some(777),
        })
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
            VideoMessage::Data(data) => {
                assert_eq!(data.codec, RtmpVideoCodec::Vp9);
                assert_eq!(data.data, Bytes::from_static(b"vp9"));
            }
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn drops_seek_commands() {
        let payload = FlvVideoData::Enhanced(ExVideoTag::StartSeek)
            .serialize()
            .unwrap();

        let message = VideoMessage::from_raw(RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id: 1,
            chunk_stream_id: 6,
            timestamp: 0,
            payload,
        })
        .unwrap();

        assert!(matches!(message, VideoMessage::Unknown));
    }

    #[test]
    fn applies_enhanced_timestamp_offset_nanos_to_h264_data() {
        let payload = FlvVideoData::Enhanced(ExVideoTag::VideoBody {
            four_cc: ExVideoFourCc::Avc1,
            packet: ExVideoPacket::CodedFrames {
                composition_time: 5,
                data: Bytes::from_static(b"frame"),
            },
            frame_type: VideoTagFrameType::Interframe,
            timestamp_offset_nanos: Some(777),
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
            VideoMessage::Data(data) => {
                assert_eq!(data.dts.as_nanos(), 100_000_777);
                assert_eq!(data.pts.as_nanos(), 105_000_777);
                assert!(!data.is_keyframe);
                assert_eq!(data.data, Bytes::from_static(b"frame"));
            }
            other => panic!("expected Data, got {other:?}"),
        }
    }
}
