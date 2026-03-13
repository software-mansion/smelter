use std::time::Duration;

use crate::{
    AacAudioConfig, AacAudioData, AudioCodec, AudioTag, AudioTagAacPacketType, AudioTagSampleSize,
    AudioTagSoundRate, GenericAudioData, RtmpMessageParseError, RtmpMessageSerializeError,
    message::AUDIO_CHUNK_STREAM_ID,
    protocol::{MessageType, RawMessage},
};

#[derive(Debug, Clone)]
pub(crate) enum AudioMessage {
    AacData(AacAudioData),
    AacConfig(AacAudioConfig),
    // Raw RTMP message for codecs that we do not explicitly support.
    Unknown(GenericAudioData),
}

impl AudioMessage {
    pub(super) fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        let tag = AudioTag::parse(msg.payload)?;
        let msg = match (tag.codec, tag.aac_packet_type) {
            (AudioCodec::Aac, Some(AudioTagAacPacketType::Data)) => Self::AacData(AacAudioData {
                pts: Duration::from_millis(msg.timestamp.into()),
                channels: tag.channels,
                data: tag.data,
            }),
            (AudioCodec::Aac, Some(AudioTagAacPacketType::Config)) => {
                Self::AacConfig(tag.data.try_into()?)
            }
            (codec, _) => Self::Unknown(GenericAudioData {
                timestamp: msg.timestamp,
                sound_rate: tag.sample_rate,
                codec,
                channels: tag.channels,
                data: tag.data,
                sample_size: Some(tag.sample_size),
            }),
        };
        Ok(msg)
    }

    pub(super) fn into_raw(self, stream_id: u32) -> Result<RawMessage, RtmpMessageSerializeError> {
        let result = match self {
            Self::AacData(chunk) => RawMessage {
                msg_type: MessageType::Audio.into_raw(),
                stream_id,
                chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
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
            Self::AacConfig(config) => RawMessage {
                msg_type: MessageType::Audio.into_raw(),
                stream_id,
                chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
                timestamp: 0,
                payload: AudioTag {
                    aac_packet_type: Some(AudioTagAacPacketType::Config),
                    codec: AudioCodec::Aac,
                    sample_rate: AudioTagSoundRate::Rate44000,
                    sample_size: AudioTagSampleSize::Sample16Bit,
                    channels: config.channels(),
                    data: config.data().clone(),
                }
                .serialize()?,
            },
            Self::Unknown(data) => RawMessage {
                msg_type: MessageType::Audio.into_raw(),
                stream_id,
                chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
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
        };
        Ok(result)
    }
}
