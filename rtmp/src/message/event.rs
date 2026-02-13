use std::time::Duration;

use crate::{
    AacAudioConfig, AacAudioData, AudioCodec, AudioTag, GenericAudioData, GenericVideoData,
    H264VideoConfig, H264VideoData, PacketType, ParseError, RtmpEvent, SampleSize,
    SerializationError, VideoCodec, VideoFrameType, VideoTag,
    message::RtmpMessage,
    protocol::{MessageType, RawMessage},
};

pub(super) fn audio_event_from_raw(msg: RawMessage) -> Result<RtmpMessage, ParseError> {
    let tag = AudioTag::parse(msg.payload)?;
    let event = match (tag.codec, tag.packet_type) {
        (AudioCodec::Aac, Some(PacketType::Data)) => RtmpEvent::AacData(AacAudioData {
            pts: Duration::from_millis(msg.timestamp.into()),
            channels: tag.channels,
            data: tag.data,
        }),
        (AudioCodec::Aac, Some(PacketType::Config)) => {
            RtmpEvent::AacConfig(AacAudioConfig {
                sample_rate: tag.sample_rate, // TODO: use correct sample rate
                channels: tag.channels,
                data: tag.data,
            })
        }
        (codec, _) => RtmpEvent::GenericAudioData(GenericAudioData {
            timestamp: msg.timestamp,
            sample_rate: tag.sample_rate,
            codec,
            channels: tag.channels,
            data: tag.data,
            sample_size: Some(tag.sample_size),
        }),
    };
    Ok(RtmpMessage::Event {
        event,
        stream_id: msg.stream_id,
    })
}

pub(super) fn video_event_from_raw(msg: RawMessage) -> Result<RtmpMessage, ParseError> {
    let tag = VideoTag::parse(msg.payload)?;
    let event = match (tag.codec, tag.packet_type) {
        (VideoCodec::H264, Some(PacketType::Data)) => RtmpEvent::H264Data(H264VideoData {
            pts: Duration::from_millis(
                (msg.timestamp as i64 + tag.composition_time.unwrap_or(0) as i64) as u64,
            ),
            dts: Duration::from_millis(msg.timestamp.into()),
            data: tag.data,
            is_keyframe: match tag.frame_type {
                VideoFrameType::Keyframe => true,
                VideoFrameType::Interframe => false,
                _ => return Err(ParseError::InvalidHeader), // TODO: better error
            },
        }),
        (VideoCodec::H264, Some(PacketType::Config)) => {
            RtmpEvent::H264Config(H264VideoConfig { data: tag.data })
        }
        (codec, _) => RtmpEvent::GenericVideoData(GenericVideoData {
            timestamp: msg.timestamp,
            codec,
            data: tag.data,
            frame_type: tag.frame_type,
        }),
    };
    Ok(RtmpMessage::Event {
        event,
        stream_id: msg.stream_id,
    })
}

pub(super) fn event_into_raw(
    event: RtmpEvent,
    stream_id: u32,
) -> Result<RawMessage, SerializationError> {
    let result = match event {
        RtmpEvent::H264Data(chunk) => RawMessage {
            msg_type: MessageType::Video,
            stream_id,
            timestamp: chunk.dts.as_millis() as u32,
            payload: VideoTag {
                packet_type: Some(PacketType::Data),
                codec: VideoCodec::H264,
                composition_time: Some(
                    (chunk.pts.as_millis() as i64 - chunk.dts.as_millis() as i64) as i32,
                ),
                frame_type: match chunk.is_keyframe {
                    true => VideoFrameType::Keyframe,
                    false => VideoFrameType::Interframe,
                },
                data: chunk.data,
            }
            .serialize()?,
        },
        RtmpEvent::H264Config(config) => RawMessage {
            msg_type: MessageType::Video,
            stream_id,
            timestamp: 0,
            payload: VideoTag {
                packet_type: Some(PacketType::Config),
                codec: VideoCodec::H264,
                composition_time: Some(0),
                frame_type: VideoFrameType::Keyframe,
                data: config.data,
            }
            .serialize()?,
        },
        RtmpEvent::AacData(chunk) => RawMessage {
            msg_type: MessageType::Video,
            stream_id,
            timestamp: chunk.pts.as_millis() as u32,
            payload: AudioTag {
                packet_type: Some(PacketType::Data),
                codec: AudioCodec::Aac,
                sample_rate: 44_100,
                sample_size: SampleSize::Sample16Bit,
                channels: chunk.channels,
                data: chunk.data,
            }
            .serialize()?,
        },
        RtmpEvent::AacConfig(config) => RawMessage {
            msg_type: MessageType::Audio,
            stream_id,
            timestamp: 0,
            payload: AudioTag {
                packet_type: Some(PacketType::Config),
                codec: AudioCodec::Aac,
                sample_rate: 44_100,
                sample_size: SampleSize::Sample16Bit,
                channels: config.channels,
                data: config.data,
            }
            .serialize()?,
        },
        RtmpEvent::GenericAudioData(data) => RawMessage {
            msg_type: MessageType::Audio,
            stream_id,
            timestamp: data.timestamp,
            payload: AudioTag {
                packet_type: None,
                codec: data.codec,
                sample_rate: data.sample_rate,
                sample_size: data.sample_size.unwrap_or(SampleSize::Sample16Bit),
                channels: data.channels,
                data: data.data,
            }
            .serialize()?,
        },
        RtmpEvent::GenericVideoData(data) => RawMessage {
            msg_type: MessageType::Video,
            stream_id,
            timestamp: data.timestamp,
            payload: VideoTag {
                packet_type: None,
                codec: data.codec,
                composition_time: None,
                frame_type: data.frame_type,
                data: data.data,
            }
            .serialize()?,
        },
        RtmpEvent::Metadata(script_data) => RawMessage {
            msg_type: MessageType::DataMessageAmf0,
            stream_id,
            timestamp: 0,
            payload: script_data.serialize()?,
        },
    };
    Ok(result)
}
