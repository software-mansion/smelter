use std::time::Duration;

use crate::{
    AacAudioData, AudioCodec, AudioFourCc, AudioTag, AudioTagAacPacketType, AudioTagSampleSize,
    AudioTagSoundRate, EnhancedAudioConfig, EnhancedAudioData, EnhancedAudioTag,
    EnhancedVideoConfig, EnhancedVideoData, EnhancedVideoTag, ExAudioPacketType, ExVideoPacketType,
    GenericAudioData, GenericVideoData, H264VideoConfig, H264VideoData, LegacyAudioTag,
    LegacyVideoTag, RtmpEvent, RtmpMessageParseError, RtmpMessageSerializeError, VideoCodec,
    VideoFourCc, VideoTag, VideoTagFrameType, VideoTagH264PacketType,
    error::FlvVideoTagParseError,
    message::{AUDIO_CHUNK_STREAM_ID, MAIN_CHUNK_STREAM_ID, RtmpMessage, VIDEO_CHUNK_STREAM_ID},
    protocol::{MessageType, RawMessage},
};

pub(super) fn audio_event_from_raw(msg: RawMessage) -> Result<RtmpMessage, RtmpMessageParseError> {
    let tag = AudioTag::parse(msg.payload.clone())?;
    let event = match tag {
        AudioTag::Legacy(tag) => legacy_audio_event(tag, &msg)?,
        AudioTag::Enhanced(tag) => enhanced_audio_event(tag, &msg)?,
    };
    Ok(RtmpMessage::Event {
        event,
        stream_id: msg.stream_id,
    })
}

fn legacy_audio_event(
    tag: LegacyAudioTag,
    msg: &RawMessage,
) -> Result<RtmpEvent, RtmpMessageParseError> {
    let event = match (tag.codec, tag.aac_packet_type) {
        (AudioCodec::Aac, Some(AudioTagAacPacketType::Data)) => RtmpEvent::AacData(AacAudioData {
            pts: Duration::from_millis(msg.timestamp.into()),
            channels: tag.channels,
            data: tag.data,
        }),
        (AudioCodec::Aac, Some(AudioTagAacPacketType::Config)) => {
            RtmpEvent::AacConfig(tag.data.try_into()?)
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
    Ok(event)
}

fn enhanced_audio_event(
    tag: EnhancedAudioTag,
    msg: &RawMessage,
) -> Result<RtmpEvent, RtmpMessageParseError> {
    // AAC via enhanced path -> emit legacy AAC events for backwards compat
    if tag.fourcc == AudioFourCc::Aac {
        return enhanced_aac_event(tag, msg);
    }

    let event = match tag.packet_type {
        ExAudioPacketType::SequenceStart => RtmpEvent::EnhancedAudioConfig(EnhancedAudioConfig {
            fourcc: tag.fourcc,
            data: tag.data,
        }),
        ExAudioPacketType::CodedFrames => RtmpEvent::EnhancedAudioData(EnhancedAudioData {
            fourcc: tag.fourcc,
            pts: Duration::from_millis(msg.timestamp.into()),
            data: tag.data,
        }),
        ExAudioPacketType::SequenceEnd => {
            // End of sequence - no meaningful event to emit, treat as metadata
            RtmpEvent::EnhancedAudioData(EnhancedAudioData {
                fourcc: tag.fourcc,
                pts: Duration::from_millis(msg.timestamp.into()),
                data: tag.data,
            })
        }
        ExAudioPacketType::MultichannelConfig | ExAudioPacketType::Multitrack => {
            // Multichannel and multitrack not yet supported, pass through as data
            RtmpEvent::EnhancedAudioData(EnhancedAudioData {
                fourcc: tag.fourcc,
                pts: Duration::from_millis(msg.timestamp.into()),
                data: tag.data,
            })
        }
    };
    Ok(event)
}

fn enhanced_aac_event(
    tag: EnhancedAudioTag,
    msg: &RawMessage,
) -> Result<RtmpEvent, RtmpMessageParseError> {
    let event = match tag.packet_type {
        ExAudioPacketType::SequenceStart => RtmpEvent::AacConfig(tag.data.try_into()?),
        ExAudioPacketType::CodedFrames => RtmpEvent::AacData(AacAudioData {
            pts: Duration::from_millis(msg.timestamp.into()),
            // Enhanced RTMP doesn't carry channel info in the tag header;
            // default to Stereo (actual channel info is in the AudioSpecificConfig)
            channels: crate::AudioChannels::Stereo,
            data: tag.data,
        }),
        _ => RtmpEvent::EnhancedAudioData(EnhancedAudioData {
            fourcc: tag.fourcc,
            pts: Duration::from_millis(msg.timestamp.into()),
            data: tag.data,
        }),
    };
    Ok(event)
}

pub(super) fn video_event_from_raw(msg: RawMessage) -> Result<RtmpMessage, RtmpMessageParseError> {
    let tag = VideoTag::parse(msg.payload.clone())?;
    let event = match tag {
        VideoTag::Legacy(tag) => legacy_video_event(tag, &msg)?,
        VideoTag::Enhanced(tag) => enhanced_video_event(tag, &msg)?,
    };
    Ok(RtmpMessage::Event {
        event,
        stream_id: msg.stream_id,
    })
}

fn legacy_video_event(
    tag: LegacyVideoTag,
    msg: &RawMessage,
) -> Result<RtmpEvent, RtmpMessageParseError> {
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
                            FlvVideoTagParseError::InvalidFrameTypeForH264(tag.frame_type).into(),
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
    Ok(event)
}

fn enhanced_video_event(
    tag: EnhancedVideoTag,
    msg: &RawMessage,
) -> Result<RtmpEvent, RtmpMessageParseError> {
    // AVC via enhanced path → emit legacy H264 events for backwards compat
    if tag.fourcc == VideoFourCc::Avc1 {
        return enhanced_avc_event(tag, msg);
    }

    let event = match tag.packet_type {
        ExVideoPacketType::SequenceStart => RtmpEvent::EnhancedVideoConfig(EnhancedVideoConfig {
            fourcc: tag.fourcc,
            data: tag.data,
        }),
        ExVideoPacketType::CodedFrames | ExVideoPacketType::CodedFramesX => {
            let composition_time = tag.composition_time.unwrap_or(0) as i64;
            let dts = Duration::from_millis(msg.timestamp.into());
            let pts = Duration::from_millis((msg.timestamp as i64 + composition_time) as u64);
            let is_keyframe = tag.frame_type == VideoTagFrameType::Keyframe;

            RtmpEvent::EnhancedVideoData(EnhancedVideoData {
                fourcc: tag.fourcc,
                pts,
                dts,
                data: tag.data,
                is_keyframe,
            })
        }
        ExVideoPacketType::SequenceEnd => {
            // End of sequence marker - emit with empty data
            RtmpEvent::EnhancedVideoData(EnhancedVideoData {
                fourcc: tag.fourcc,
                pts: Duration::from_millis(msg.timestamp.into()),
                dts: Duration::from_millis(msg.timestamp.into()),
                data: tag.data,
                is_keyframe: false,
            })
        }
        ExVideoPacketType::Metadata => {
            // Enhanced video metadata (e.g., HDR colorInfo) - pass through as data
            RtmpEvent::EnhancedVideoData(EnhancedVideoData {
                fourcc: tag.fourcc,
                pts: Duration::from_millis(msg.timestamp.into()),
                dts: Duration::from_millis(msg.timestamp.into()),
                data: tag.data,
                is_keyframe: false,
            })
        }
    };
    Ok(event)
}

fn enhanced_avc_event(
    tag: EnhancedVideoTag,
    msg: &RawMessage,
) -> Result<RtmpEvent, RtmpMessageParseError> {
    let event = match tag.packet_type {
        ExVideoPacketType::SequenceStart => {
            RtmpEvent::H264Config(H264VideoConfig { data: tag.data })
        }
        ExVideoPacketType::CodedFrames | ExVideoPacketType::CodedFramesX => {
            let composition_time = tag.composition_time.unwrap_or(0) as i64;
            let dts = Duration::from_millis(msg.timestamp.into());
            let pts = Duration::from_millis((msg.timestamp as i64 + composition_time) as u64);
            let is_keyframe = match tag.frame_type {
                VideoTagFrameType::Keyframe => true,
                VideoTagFrameType::Interframe => false,
                _ => {
                    return Err(
                        FlvVideoTagParseError::InvalidFrameTypeForH264(tag.frame_type).into(),
                    );
                }
            };
            RtmpEvent::H264Data(H264VideoData {
                pts,
                dts,
                data: tag.data,
                is_keyframe,
            })
        }
        _ => RtmpEvent::EnhancedVideoData(EnhancedVideoData {
            fourcc: tag.fourcc,
            pts: Duration::from_millis(msg.timestamp.into()),
            dts: Duration::from_millis(msg.timestamp.into()),
            data: tag.data,
            is_keyframe: false,
        }),
    };
    Ok(event)
}

pub(super) fn event_into_raw(
    event: RtmpEvent,
    stream_id: u32,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    let result = match event {
        RtmpEvent::H264Data(chunk) => RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id,
            chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
            timestamp: chunk.dts.as_millis() as u32,
            payload: VideoTag::Legacy(LegacyVideoTag {
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
        RtmpEvent::H264Config(config) => RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id,
            chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
            timestamp: 0,
            payload: VideoTag::Legacy(LegacyVideoTag {
                h264_packet_type: Some(VideoTagH264PacketType::Config),
                codec: VideoCodec::H264,
                composition_time: Some(0),
                frame_type: VideoTagFrameType::Keyframe,
                data: config.data,
            })
            .serialize()?,
        },
        RtmpEvent::AacData(chunk) => RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id,
            chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
            timestamp: chunk.pts.as_millis() as u32,
            payload: AudioTag::Legacy(LegacyAudioTag {
                aac_packet_type: Some(AudioTagAacPacketType::Data),
                codec: AudioCodec::Aac,
                sample_rate: AudioTagSoundRate::Rate44000,
                sample_size: AudioTagSampleSize::Sample16Bit,
                channels: chunk.channels,
                data: chunk.data,
            })
            .serialize()?,
        },
        RtmpEvent::AacConfig(config) => RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id,
            chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
            timestamp: 0,
            payload: AudioTag::Legacy(LegacyAudioTag {
                aac_packet_type: Some(AudioTagAacPacketType::Config),
                codec: AudioCodec::Aac,
                sample_rate: AudioTagSoundRate::Rate44000,
                sample_size: AudioTagSampleSize::Sample16Bit,
                channels: config.channels(),
                data: config.data().clone(),
            })
            .serialize()?,
        },
        RtmpEvent::GenericVideoData(data) => RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id,
            chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
            timestamp: data.timestamp,
            payload: VideoTag::Legacy(LegacyVideoTag {
                h264_packet_type: None,
                codec: data.codec,
                composition_time: None,
                frame_type: data.frame_type,
                data: data.data,
            })
            .serialize()?,
        },
        RtmpEvent::GenericAudioData(data) => RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id,
            chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
            timestamp: data.timestamp,
            payload: AudioTag::Legacy(LegacyAudioTag {
                aac_packet_type: None,
                codec: data.codec,
                sample_rate: data.sound_rate,
                sample_size: data.sample_size.unwrap_or(AudioTagSampleSize::Sample16Bit),
                channels: data.channels,
                data: data.data,
            })
            .serialize()?,
        },
        RtmpEvent::EnhancedVideoData(data) => RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id,
            chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
            timestamp: data.dts.as_millis() as u32,
            payload: VideoTag::Enhanced(EnhancedVideoTag {
                frame_type: if data.is_keyframe {
                    VideoTagFrameType::Keyframe
                } else {
                    VideoTagFrameType::Interframe
                },
                fourcc: data.fourcc,
                packet_type: if data.pts != data.dts {
                    ExVideoPacketType::CodedFrames
                } else {
                    ExVideoPacketType::CodedFramesX
                },
                composition_time: if data.pts != data.dts {
                    Some((data.pts.as_millis() as i64 - data.dts.as_millis() as i64) as i32)
                } else {
                    None
                },
                data: data.data,
            })
            .serialize()?,
        },
        RtmpEvent::EnhancedVideoConfig(config) => RawMessage {
            msg_type: MessageType::Video.into_raw(),
            stream_id,
            chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
            timestamp: 0,
            payload: VideoTag::Enhanced(EnhancedVideoTag {
                frame_type: VideoTagFrameType::Keyframe,
                fourcc: config.fourcc,
                packet_type: ExVideoPacketType::SequenceStart,
                composition_time: None,
                data: config.data,
            })
            .serialize()?,
        },
        RtmpEvent::EnhancedAudioData(data) => RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id,
            chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
            timestamp: data.pts.as_millis() as u32,
            payload: AudioTag::Enhanced(EnhancedAudioTag {
                fourcc: data.fourcc,
                packet_type: ExAudioPacketType::CodedFrames,
                data: data.data,
            })
            .serialize()?,
        },
        RtmpEvent::EnhancedAudioConfig(config) => RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id,
            chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
            timestamp: 0,
            payload: AudioTag::Enhanced(EnhancedAudioTag {
                fourcc: config.fourcc,
                packet_type: ExAudioPacketType::SequenceStart,
                data: config.data,
            })
            .serialize()?,
        },
        RtmpEvent::Metadata(script_data) => RawMessage {
            msg_type: MessageType::DataMessageAmf0.into_raw(),
            stream_id,
            chunk_stream_id: MAIN_CHUNK_STREAM_ID,
            timestamp: 0,
            payload: script_data.serialize()?,
        },
    };
    Ok(result)
}
