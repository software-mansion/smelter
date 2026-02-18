use std::time::Duration;

use crate::{
    AacAudioConfig, AacAudioData, AudioCodec, AudioTag, AudioTagAacPacketType, AudioTagSampleSize,
    AudioTagSoundRate, GenericAudioData, GenericVideoData, H264VideoConfig, H264VideoData,
    ParseError, RtmpEvent, SerializationError, VideoCodec, VideoTag, VideoTagFrameType,
    VideoTagH264PacketType, VideoTagParseError,
    message::RtmpMessage,
    protocol::{MessageType, RawMessage},
};

use tracing::error;

pub(super) fn audio_event_from_raw(msg: RawMessage) -> Result<RtmpMessage, ParseError> {
    let tag = AudioTag::parse(msg.payload)?;
    let event = match (tag.codec, tag.aac_packet_type) {
        (AudioCodec::Aac, Some(AudioTagAacPacketType::Data)) => RtmpEvent::AacData(AacAudioData {
            pts: Duration::from_millis(msg.timestamp.into()),
            channels: tag.channels,
            data: tag.data,
        }),
        (AudioCodec::Aac, Some(AudioTagAacPacketType::Config)) => {
            RtmpEvent::AacConfig(AacAudioConfig::new(tag.data))
        }
        (codec, _) => RtmpEvent::GenericAudioData(GenericAudioData {
            timestamp: msg.timestamp,
            sound_rate: tag.sample_rate,
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
    let event = match (tag.codec, tag.h264_packet_type) {
        (VideoCodec::H264, Some(VideoTagH264PacketType::Data)) => {
            RtmpEvent::H264Data(H264VideoData {
                pts: Duration::from_millis(
                    (msg.timestamp as i64 + tag.composition_time.unwrap_or(0) as i64) as u64,
                ),
                dts: Duration::from_millis(msg.timestamp.into()),
                data: tag.data,
                is_keyframe: match tag.frame_type {
                    VideoTagFrameType::Keyframe => true,
                    VideoTagFrameType::Interframe => false,
                    _ => {
                        return Err(
                            VideoTagParseError::InvalidFrameTypeForH264(tag.frame_type).into()
                        );
                    }
                },
            })
        }
        (VideoCodec::H264, Some(VideoTagH264PacketType::Config)) => {
            RtmpEvent::H264Config(H264VideoConfig { data: tag.data })
        }
        // TODO
        // (VideoCodec::H264, Some(VideoTagH264PacketType::Eos)) => {

        // }
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
            }
            .serialize()?,
        },
        RtmpEvent::H264Config(config) => RawMessage {
            msg_type: MessageType::Video,
            stream_id,
            timestamp: 0,
            payload: VideoTag {
                h264_packet_type: Some(VideoTagH264PacketType::Config),
                codec: VideoCodec::H264,
                composition_time: Some(0),
                frame_type: VideoTagFrameType::Keyframe,
                data: config.data,
            }
            .serialize()?,
        },
        RtmpEvent::AacData(chunk) => RawMessage {
            msg_type: MessageType::Audio,
            stream_id,
            timestamp: chunk.pts.as_millis() as u32,
            payload: AudioTag {
                aac_packet_type: Some(AudioTagAacPacketType::Data),
                codec: AudioCodec::Aac,
                sample_rate: AudioTagSoundRate::Rate44000,
                sample_size: AudioTagSampleSize::Sample16Bit,
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
                aac_packet_type: Some(AudioTagAacPacketType::Config),
                codec: AudioCodec::Aac,
                sample_rate: AudioTagSoundRate::Rate44000,
                sample_size: AudioTagSampleSize::Sample16Bit,
                channels: config
                    .channels()
                    .map_err(|_| SerializationError::AscParseError)?,
                data: config.data().clone(),
            }
            .serialize()?,
        },
        RtmpEvent::GenericAudioData(data) => RawMessage {
            msg_type: MessageType::Audio,
            stream_id,
            timestamp: data.timestamp,
            payload: AudioTag {
                aac_packet_type: None,
                codec: data.codec,
                sample_rate: data.sound_rate,
                sample_size: data.sample_size.unwrap_or(AudioTagSampleSize::Sample16Bit),
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
                h264_packet_type: None,
                codec: data.codec,
                composition_time: None,
                frame_type: data.frame_type,
                data: data.data,
            }
            .serialize()?,
        },
        // TODO: (@jbrs) This should depend on the encoding
        RtmpEvent::Metadata(script_data) => RawMessage {
            msg_type: MessageType::DataMessageAmf0,
            stream_id,
            timestamp: 0,
            payload: script_data.serialize()?,
        },
    };
    Ok(result)
}
