use std::time::Duration;

use bytes::Bytes;

use crate::{
    AacAudioData, Ac3AudioConfig, Ac3AudioData, AudioCodec, AudioFourCc, AudioTag,
    AudioTagAacPacketType, AudioTagSampleSize, AudioTagSoundRate, Av1VideoConfig, Av1VideoData,
    Eac3AudioConfig, Eac3AudioData, EnhancedAudioTag, EnhancedVideoTag, ExAudioPacketType,
    ExVideoPacketType, FlacAudioConfig, FlacAudioData, GenericAudioData, GenericVideoData,
    H264VideoConfig, H264VideoData, HevcVideoConfig, HevcVideoData, LegacyAudioTag, LegacyVideoTag,
    Mp3AudioConfig, Mp3AudioData, OpusAudioConfig, OpusAudioData, RtmpEvent, RtmpMessageParseError,
    RtmpMessageSerializeError, VideoCodec, VideoFourCc, VideoTag, VideoTagFrameType,
    VideoTagH264PacketType, Vp9VideoConfig, Vp9VideoData,
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
    // AAC already exists in legacy RTMP (sound format 10). Enhanced RTMP added a second way
    // to signal it via FourCC (mp4a). Map to the same AacData/AacConfig events as the legacy path.
    if tag.fourcc == AudioFourCc::Aac {
        return enhanced_aac_event(tag, msg);
    }

    let pts = Duration::from_millis(msg.timestamp.into());

    let event = match (tag.fourcc, tag.packet_type) {
        (AudioFourCc::Opus, ExAudioPacketType::SequenceStart) => {
            RtmpEvent::OpusConfig(OpusAudioConfig { data: tag.data })
        }
        (AudioFourCc::Opus, _) => RtmpEvent::OpusData(OpusAudioData {
            pts,
            data: tag.data,
        }),
        (AudioFourCc::Flac, ExAudioPacketType::SequenceStart) => {
            RtmpEvent::FlacConfig(FlacAudioConfig { data: tag.data })
        }
        (AudioFourCc::Flac, _) => RtmpEvent::FlacData(FlacAudioData {
            pts,
            data: tag.data,
        }),
        (AudioFourCc::Mp3, ExAudioPacketType::SequenceStart) => {
            RtmpEvent::Mp3Config(Mp3AudioConfig { data: tag.data })
        }
        (AudioFourCc::Mp3, _) => RtmpEvent::Mp3Data(Mp3AudioData {
            pts,
            data: tag.data,
        }),
        (AudioFourCc::Ac3, ExAudioPacketType::SequenceStart) => {
            RtmpEvent::Ac3Config(Ac3AudioConfig { data: tag.data })
        }
        (AudioFourCc::Ac3, _) => RtmpEvent::Ac3Data(Ac3AudioData {
            pts,
            data: tag.data,
        }),
        (AudioFourCc::Eac3, ExAudioPacketType::SequenceStart) => {
            RtmpEvent::Eac3Config(Eac3AudioConfig { data: tag.data })
        }
        (AudioFourCc::Eac3, _) => RtmpEvent::Eac3Data(Eac3AudioData {
            pts,
            data: tag.data,
        }),
        // AAC is handled above, this arm is unreachable but needed for exhaustiveness
        (AudioFourCc::Aac, _) => unreachable!(),
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
        _ => RtmpEvent::AacData(AacAudioData {
            pts: Duration::from_millis(msg.timestamp.into()),
            channels: crate::AudioChannels::Stereo,
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
    // AVC/H.264 already exists in legacy RTMP (codec ID 7). Enhanced RTMP added a second way
    // to signal it via FourCC (avc1). Map to the same H264Data/H264Config events as the legacy path.
    if tag.fourcc == VideoFourCc::Avc1 {
        return enhanced_avc_event(tag, msg);
    }

    let composition_time = tag.composition_time.unwrap_or(0) as i64;
    let dts = Duration::from_millis(msg.timestamp.into());
    let pts = Duration::from_millis((msg.timestamp as i64 + composition_time) as u64);
    let is_keyframe = tag.frame_type == VideoTagFrameType::Keyframe;

    let event = match (tag.fourcc, tag.packet_type) {
        (VideoFourCc::Hvc1, ExVideoPacketType::SequenceStart) => {
            RtmpEvent::HevcConfig(HevcVideoConfig { data: tag.data })
        }
        (VideoFourCc::Hvc1, _) => RtmpEvent::HevcData(HevcVideoData {
            pts,
            dts,
            data: tag.data,
            is_keyframe,
        }),
        (VideoFourCc::Av01, ExVideoPacketType::SequenceStart) => {
            RtmpEvent::Av1Config(Av1VideoConfig { data: tag.data })
        }
        (VideoFourCc::Av01, _) => RtmpEvent::Av1Data(Av1VideoData {
            pts,
            dts,
            data: tag.data,
            is_keyframe,
        }),
        (VideoFourCc::Vp09, ExVideoPacketType::SequenceStart) => {
            RtmpEvent::Vp9Config(Vp9VideoConfig { data: tag.data })
        }
        (VideoFourCc::Vp09, _) => RtmpEvent::Vp9Data(Vp9VideoData {
            pts,
            dts,
            data: tag.data,
            is_keyframe,
        }),
        // AVC is handled above, this arm is unreachable but needed for exhaustiveness
        (VideoFourCc::Avc1, _) => unreachable!(),
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
        _ => RtmpEvent::H264Data(H264VideoData {
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

        // Enhanced video codecs
        RtmpEvent::HevcData(data) => enhanced_video_data_to_raw(
            VideoFourCc::Hvc1,
            data.pts,
            data.dts,
            data.data,
            data.is_keyframe,
            stream_id,
        )?,
        RtmpEvent::HevcConfig(config) => {
            enhanced_video_config_to_raw(VideoFourCc::Hvc1, config.data, stream_id)?
        }
        RtmpEvent::Av1Data(data) => enhanced_video_data_to_raw(
            VideoFourCc::Av01,
            data.pts,
            data.dts,
            data.data,
            data.is_keyframe,
            stream_id,
        )?,
        RtmpEvent::Av1Config(config) => {
            enhanced_video_config_to_raw(VideoFourCc::Av01, config.data, stream_id)?
        }
        RtmpEvent::Vp9Data(data) => enhanced_video_data_to_raw(
            VideoFourCc::Vp09,
            data.pts,
            data.dts,
            data.data,
            data.is_keyframe,
            stream_id,
        )?,
        RtmpEvent::Vp9Config(config) => {
            enhanced_video_config_to_raw(VideoFourCc::Vp09, config.data, stream_id)?
        }

        // Enhanced audio codecs
        RtmpEvent::OpusData(data) => {
            enhanced_audio_data_to_raw(AudioFourCc::Opus, data.pts, data.data, stream_id)?
        }
        RtmpEvent::OpusConfig(config) => {
            enhanced_audio_config_to_raw(AudioFourCc::Opus, config.data, stream_id)?
        }
        RtmpEvent::FlacData(data) => {
            enhanced_audio_data_to_raw(AudioFourCc::Flac, data.pts, data.data, stream_id)?
        }
        RtmpEvent::FlacConfig(config) => {
            enhanced_audio_config_to_raw(AudioFourCc::Flac, config.data, stream_id)?
        }
        RtmpEvent::Mp3Data(data) => {
            enhanced_audio_data_to_raw(AudioFourCc::Mp3, data.pts, data.data, stream_id)?
        }
        RtmpEvent::Mp3Config(config) => {
            enhanced_audio_config_to_raw(AudioFourCc::Mp3, config.data, stream_id)?
        }
        RtmpEvent::Ac3Data(data) => {
            enhanced_audio_data_to_raw(AudioFourCc::Ac3, data.pts, data.data, stream_id)?
        }
        RtmpEvent::Ac3Config(config) => {
            enhanced_audio_config_to_raw(AudioFourCc::Ac3, config.data, stream_id)?
        }
        RtmpEvent::Eac3Data(data) => {
            enhanced_audio_data_to_raw(AudioFourCc::Eac3, data.pts, data.data, stream_id)?
        }
        RtmpEvent::Eac3Config(config) => {
            enhanced_audio_config_to_raw(AudioFourCc::Eac3, config.data, stream_id)?
        }

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

fn enhanced_video_data_to_raw(
    fourcc: VideoFourCc,
    pts: Duration,
    dts: Duration,
    data: Bytes,
    is_keyframe: bool,
    stream_id: u32,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    Ok(RawMessage {
        msg_type: MessageType::Video.into_raw(),
        stream_id,
        chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
        timestamp: dts.as_millis() as u32,
        payload: VideoTag::Enhanced(EnhancedVideoTag {
            frame_type: if is_keyframe {
                VideoTagFrameType::Keyframe
            } else {
                VideoTagFrameType::Interframe
            },
            fourcc,
            packet_type: if pts != dts {
                ExVideoPacketType::CodedFrames
            } else {
                ExVideoPacketType::CodedFramesX
            },
            composition_time: if pts != dts {
                Some((pts.as_millis() as i64 - dts.as_millis() as i64) as i32)
            } else {
                None
            },
            data,
        })
        .serialize()?,
    })
}

fn enhanced_video_config_to_raw(
    fourcc: VideoFourCc,
    data: Bytes,
    stream_id: u32,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    Ok(RawMessage {
        msg_type: MessageType::Video.into_raw(),
        stream_id,
        chunk_stream_id: VIDEO_CHUNK_STREAM_ID,
        timestamp: 0,
        payload: VideoTag::Enhanced(EnhancedVideoTag {
            frame_type: VideoTagFrameType::Keyframe,
            fourcc,
            packet_type: ExVideoPacketType::SequenceStart,
            composition_time: None,
            data,
        })
        .serialize()?,
    })
}

fn enhanced_audio_data_to_raw(
    fourcc: AudioFourCc,
    pts: Duration,
    data: Bytes,
    stream_id: u32,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    Ok(RawMessage {
        msg_type: MessageType::Audio.into_raw(),
        stream_id,
        chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
        timestamp: pts.as_millis() as u32,
        payload: AudioTag::Enhanced(EnhancedAudioTag {
            fourcc,
            packet_type: ExAudioPacketType::CodedFrames,
            data,
        })
        .serialize()?,
    })
}

fn enhanced_audio_config_to_raw(
    fourcc: AudioFourCc,
    data: Bytes,
    stream_id: u32,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    Ok(RawMessage {
        msg_type: MessageType::Audio.into_raw(),
        stream_id,
        chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
        timestamp: 0,
        payload: AudioTag::Enhanced(EnhancedAudioTag {
            fourcc,
            packet_type: ExAudioPacketType::SequenceStart,
            data,
        })
        .serialize()?,
    })
}
