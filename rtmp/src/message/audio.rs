use std::time::Duration;

use tracing::warn;

use crate::{
    AacAudioConfig, AudioChannels, AudioConfig, AudioData, AudioTag, AudioTagAacPacketType,
    AudioTagSampleSize, AudioTagSoundRate, FlvAudioTagParseError, LegacyFlvAudioCodec,
    RtmpAudioCodec, RtmpMessageParseError, RtmpMessageSerializeError, TrackId,
    message::AUDIO_CHUNK_STREAM_ID,
    protocol::{MessageType, RawMessage},
};

#[derive(Debug, Clone)]
pub(crate) enum AudioMessage {
    Data(AudioData),
    Config(AudioConfig),
    /// Wire-level audio packet types that carry no user-visible payload
    /// (Enhanced RTMP ExHeader audio — parsing not yet implemented).
    Unknown,
}

impl AudioMessage {
    pub(crate) fn is_media_packet(&self) -> bool {
        matches!(self, Self::Data(_))
    }

    pub(super) fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        let tag = match AudioTag::parse(msg.payload) {
            Ok(tag) => tag,
            Err(FlvAudioTagParseError::UnknownCodecId(_)) => {
                return Ok(Self::Unknown);
            }
            Err(err) => return Err(err.into()),
        };

        let codec = match RtmpAudioCodec::try_from(tag.codec) {
            Ok(codec) => codec,
            Err(err) => {
                warn!("{err}. Returning Unknown.");
                return Ok(Self::Unknown);
            }
        };

        let pts = Duration::from_millis(msg.timestamp.into());

        let msg = match (codec, tag.aac_packet_type) {
            (RtmpAudioCodec::Aac, Some(AudioTagAacPacketType::Config)) => {
                Self::Config(AudioConfig {
                    track_id: TrackId::PRIMARY,
                    codec,
                    data: tag.data,
                })
            }
            _ => Self::Data(AudioData {
                track_id: TrackId::PRIMARY,
                codec,
                pts,
                channels: tag.channels,
                data: tag.data,
            }),
        };
        Ok(msg)
    }

    pub(super) fn into_raw(self, stream_id: u32) -> Result<RawMessage, RtmpMessageSerializeError> {
        match self {
            Self::Data(audio) => {
                let legacy_codec: LegacyFlvAudioCodec = audio.codec.try_into()?;
                let aac_packet_type = match legacy_codec {
                    LegacyFlvAudioCodec::Aac => Some(AudioTagAacPacketType::Data),
                    _ => None,
                };
                Ok(RawMessage {
                    msg_type: MessageType::Audio.into_raw(),
                    stream_id,
                    chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
                    timestamp: audio.pts.as_millis() as u32,
                    payload: AudioTag {
                        aac_packet_type,
                        codec: legacy_codec,
                        sample_rate: AudioTagSoundRate::Rate44000,
                        sample_size: AudioTagSampleSize::Sample16Bit,
                        channels: audio.channels,
                        data: audio.data,
                    }
                    .serialize()?,
                })
            }
            Self::Unknown => Err(RtmpMessageSerializeError::InternalError(
                "Cannot serialize an unknown audio message".into(),
            )),
            Self::Config(config) => {
                let legacy_codec: LegacyFlvAudioCodec = config.codec.try_into()?;
                let (aac_packet_type, channels) = match legacy_codec {
                    LegacyFlvAudioCodec::Aac => {
                        let parsed =
                            AacAudioConfig::try_from(config.data.clone()).map_err(|err| {
                                RtmpMessageSerializeError::InternalError(format!(
                                    "Failed to parse AAC config: {err}"
                                ))
                            })?;
                        (Some(AudioTagAacPacketType::Config), parsed.channels())
                    }
                    _ => (None, AudioChannels::Stereo),
                };
                Ok(RawMessage {
                    msg_type: MessageType::Audio.into_raw(),
                    stream_id,
                    chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
                    timestamp: 0,
                    payload: AudioTag {
                        aac_packet_type,
                        codec: legacy_codec,
                        sample_rate: AudioTagSoundRate::Rate44000,
                        sample_size: AudioTagSampleSize::Sample16Bit,
                        channels,
                        data: config.data,
                    }
                    .serialize()?,
                })
            }
        }
    }
}
