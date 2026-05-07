use std::time::Duration;

use tracing::warn;

use crate::{
    AacAudioConfig, AudioChannels, AudioConfig, AudioData, AudioTag, AudioTagAacPacketType,
    AudioTagSampleSize, AudioTagSoundRate, ExAudioPacket, ExAudioTag, FlvAudioData,
    LegacyFlvAudioCodec, RtmpAudioCodec, RtmpMessageParseError, RtmpMessageSerializeError, TrackId,
    message::{AUDIO_CHUNK_STREAM_ID, SenderState, TrackKey},
    protocol::{MessageType, RawMessage},
};

#[derive(Debug, Clone)]
pub(crate) enum AudioMessage {
    Data(AudioData),
    Config(AudioConfig),
    /// Wire-level audio packet types that carry no user-visible payload.
    Unknown,
}

impl AudioMessage {
    pub(crate) fn is_media_packet(&self) -> bool {
        matches!(self, Self::Data(_))
    }

    pub(super) fn from_raw(msg: RawMessage) -> Result<Self, RtmpMessageParseError> {
        match FlvAudioData::parse(msg.payload)? {
            FlvAudioData::Legacy(tag) => Self::from_legacy(msg.timestamp, tag),
            FlvAudioData::Enhanced(tag) => Self::from_enhanced(msg.timestamp, tag),
        }
    }

    fn from_legacy(timestamp: u32, tag: AudioTag) -> Result<Self, RtmpMessageParseError> {
        let codec = match RtmpAudioCodec::try_from(tag.codec) {
            Ok(codec) => codec,
            Err(err) => {
                warn!("{err}. Returning Unknown.");
                return Ok(Self::Unknown);
            }
        };

        match (codec, tag.aac_packet_type) {
            (RtmpAudioCodec::Aac, Some(AudioTagAacPacketType::Config)) => {
                let channels = AacAudioConfig::try_from(tag.data.clone())?.channels();
                Ok(Self::Config(AudioConfig {
                    track_id: TrackId::PRIMARY,
                    codec,
                    data: tag.data,
                    channels,
                }))
            }
            _ => {
                // Data-class messages carry no channel info on the wire. For AAC the
                // legacy SoundType bit must be ignored (FLV v10.1 §E.4.2.1 line 3364:
                // "Flash Player ignores SoundRate/SoundType for AAC and uses values
                // from AudioSpecificConfig"). Channels flow out via AudioConfig.channels.
                Ok(Self::Data(AudioData {
                    track_id: TrackId::PRIMARY,
                    codec,
                    pts: Duration::from_millis(timestamp.into()),
                    data: tag.data,
                }))
            }
        }
    }

    fn from_enhanced(timestamp: u32, tag: ExAudioTag) -> Result<Self, RtmpMessageParseError> {
        let codec = match RtmpAudioCodec::try_from(tag.four_cc) {
            Ok(codec) => codec,
            Err(err) => {
                warn!("{err}. Returning Unknown.");
                return Ok(Self::Unknown);
            }
        };

        match tag.packet {
            ExAudioPacket::SequenceStart(data) => {
                let channels = match codec {
                    RtmpAudioCodec::Aac => AacAudioConfig::try_from(data.clone())?.channels(),
                };
                Ok(Self::Config(AudioConfig {
                    track_id: TrackId::PRIMARY,
                    codec,
                    data,
                    channels,
                }))
            }
            ExAudioPacket::CodedFrames(data) => Ok(Self::Data(AudioData {
                track_id: TrackId::PRIMARY,
                codec,
                pts: Duration::from_millis(timestamp.into())
                    + Duration::from_nanos(u64::from(tag.timestamp_offset_nanos.unwrap_or(0))),
                data,
            })),
            ExAudioPacket::SequenceEnd | ExAudioPacket::MultichannelConfig(_) => Ok(Self::Unknown),
        }
    }

    pub(super) fn into_raw(
        self,
        stream_id: u32,
        state: &mut SenderState,
    ) -> Result<RawMessage, RtmpMessageSerializeError> {
        match self {
            Self::Data(audio) => {
                let legacy_codec: LegacyFlvAudioCodec = audio.codec.try_into()?;
                let aac_packet_type = match legacy_codec {
                    LegacyFlvAudioCodec::Aac => Some(AudioTagAacPacketType::Data),
                    _ => None,
                };
                let channels = state
                    .audio_channels
                    .get(&TrackKey::new(stream_id, audio.track_id))
                    .copied()
                    .unwrap_or(AudioChannels::Stereo);
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
                        channels,
                        data: audio.data,
                    }
                    .serialize()?,
                })
            }
            Self::Unknown => Err(RtmpMessageSerializeError::InternalError(
                "Cannot serialize an unknown audio message".into(),
            )),
            Self::Config(config) => {
                state
                    .audio_channels
                    .insert(TrackKey::new(stream_id, config.track_id), config.channels);
                let legacy_codec: LegacyFlvAudioCodec = config.codec.try_into()?;
                let (aac_packet_type, channels) = match legacy_codec {
                    LegacyFlvAudioCodec::Aac => {
                        (Some(AudioTagAacPacketType::Config), config.channels)
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;

    use super::AudioMessage;
    use crate::{
        AudioChannels, AudioConfig, AudioData, RtmpAudioCodec, TrackId,
        message::SenderState,
        protocol::{MessageType, RawMessage},
    };

    #[test]
    fn parses_enhanced_aac_sequence_start_as_config() {
        let payload = Bytes::from_static(&[
            0x90, // [soundFormat=9 ExHeader | packetType=0 SequenceStart]
            b'm', b'p', b'4', b'a', 0x12, 0x10,
        ]);

        let message = AudioMessage::from_raw(RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id: 1,
            chunk_stream_id: 4,
            timestamp: 0,
            payload,
        })
        .unwrap();

        match message {
            AudioMessage::Config(config) => {
                assert_eq!(config.codec, RtmpAudioCodec::Aac);
                assert_eq!(config.data, Bytes::from_static(&[0x12, 0x10]));
                assert_eq!(config.channels, AudioChannels::Stereo);
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn parses_enhanced_aac_coded_frames_with_nano_offset() {
        let payload = Bytes::from_static(&[
            0x97, // [soundFormat=9 ExHeader | packetType=7 ModEx]
            2, 0x00, 0x00, 0x64, // ModEx data size=3, TimestampOffsetNano=100
            0x01, // [modExType=0 TimestampOffsetNano | next packetType=1 CodedFrames]
            b'm', b'p', b'4', b'a', b'f', b'r', b'a', b'm', b'e',
        ]);

        let message = AudioMessage::from_raw(RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id: 1,
            chunk_stream_id: 4,
            timestamp: 123,
            payload,
        })
        .unwrap();

        match message {
            AudioMessage::Data(data) => {
                assert_eq!(data.codec, RtmpAudioCodec::Aac);
                assert_eq!(data.pts.as_nanos(), 123_000_100);
                assert_eq!(data.data, Bytes::from_static(b"frame"));
            }
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_enhanced_codec_maps_to_unknown() {
        let payload = Bytes::from_static(&[
            0x91, // [soundFormat=9 ExHeader | packetType=1 CodedFrames]
            b'O', b'p', b'u', b's', 0xF0,
        ]);

        let message = AudioMessage::from_raw(RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id: 1,
            chunk_stream_id: 4,
            timestamp: 10,
            payload,
        })
        .unwrap();

        assert!(matches!(message, AudioMessage::Unknown));
    }

    #[test]
    fn multichannel_config_maps_to_unknown() {
        let payload = Bytes::from_static(&[
            0x94, // [soundFormat=9 ExHeader | packetType=4 MultichannelConfig]
            b'm', b'p', b'4', b'a', 0x00, 0x02,
        ]);

        let message = AudioMessage::from_raw(RawMessage {
            msg_type: MessageType::Audio.into_raw(),
            stream_id: 1,
            chunk_stream_id: 4,
            timestamp: 10,
            payload,
        })
        .unwrap();

        assert!(matches!(message, AudioMessage::Unknown));
    }

    #[test]
    fn serializes_legacy_audio_data_using_stream_and_track_specific_state() {
        let mut state = SenderState::default();

        AudioMessage::Config(AudioConfig {
            track_id: TrackId(1),
            codec: RtmpAudioCodec::Aac,
            data: Bytes::from_static(&[0x12, 0x10]),
            channels: AudioChannels::Mono,
        })
        .into_raw(1, &mut state)
        .unwrap();

        AudioMessage::Config(AudioConfig {
            track_id: TrackId(1),
            codec: RtmpAudioCodec::Aac,
            data: Bytes::from_static(&[0x12, 0x10]),
            channels: AudioChannels::Stereo,
        })
        .into_raw(2, &mut state)
        .unwrap();

        let raw = AudioMessage::Data(AudioData {
            track_id: TrackId(1),
            codec: RtmpAudioCodec::Aac,
            pts: Duration::from_millis(33),
            data: Bytes::from_static(b"frame"),
        })
        .into_raw(1, &mut state)
        .unwrap();

        assert_eq!(raw.payload[0] & 0b0000_0001, 0);

        let raw = AudioMessage::Data(AudioData {
            track_id: TrackId(1),
            codec: RtmpAudioCodec::Aac,
            pts: Duration::from_millis(33),
            data: Bytes::from_static(b"frame"),
        })
        .into_raw(2, &mut state)
        .unwrap();

        assert_eq!(raw.payload[0] & 0b0000_0001, 1);
    }
}
