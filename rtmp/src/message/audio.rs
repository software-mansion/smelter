use std::time::Duration;

use tracing::warn;

use crate::{
    AacAudioConfig, AudioChannels, AudioConfig, AudioData, AudioTag, AudioTagAacPacketType,
    AudioTagSampleSize, AudioTagSoundRate, ExAudioFourCc, ExAudioPacket, ExAudioTag, FlvAudioData,
    LegacyFlvAudioCodec, OpusAudioConfig, RtmpAudioCodec, RtmpMessageParseError,
    RtmpMessageSerializeError, TrackId,
    message::AUDIO_CHUNK_STREAM_ID,
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
                    RtmpAudioCodec::Opus => OpusAudioConfig::try_from(data.clone())
                        .inspect_err(|err| {
                            warn!("Failed to parse Opus ID header, defaulting to stereo: {err}")
                        })
                        .map(|c| c.channels())
                        .unwrap_or(AudioChannels::Stereo),
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
        channels: AudioChannels,
    ) -> Result<RawMessage, RtmpMessageSerializeError> {
        match self {
            Self::Data(audio) => audio_data_into_raw(audio, stream_id, channels),
            Self::Config(config) => audio_config_into_raw(config, stream_id),
            Self::Unknown => Err(RtmpMessageSerializeError::InternalError(
                "Cannot serialize an unknown audio message".into(),
            )),
        }
    }
}

fn audio_data_into_raw(
    audio: AudioData,
    stream_id: u32,
    channels: AudioChannels,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    let payload = match audio.codec {
        RtmpAudioCodec::Aac => FlvAudioData::Legacy(AudioTag {
            aac_packet_type: Some(AudioTagAacPacketType::Data),
            codec: LegacyFlvAudioCodec::Aac,
            sample_rate: AudioTagSoundRate::Rate44000,
            sample_size: AudioTagSampleSize::Sample16Bit,
            channels,
            data: audio.data,
        })
        .serialize()?,
        RtmpAudioCodec::Opus => {
            let pts_nanos = audio.pts.as_nanos();
            let timestamp_offset_nanos = match (pts_nanos % 1_000_000) as u32 {
                0 => None,
                offset => Some(offset),
            };
            FlvAudioData::Enhanced(ExAudioTag {
                four_cc: ExAudioFourCc::from(audio.codec),
                packet: ExAudioPacket::CodedFrames(audio.data),
                timestamp_offset_nanos,
            })
            .serialize()?
        }
    };

    Ok(RawMessage {
        msg_type: MessageType::Audio.into_raw(),
        stream_id,
        chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
        timestamp: audio.pts.as_millis() as u32,
        payload,
    })
}

fn audio_config_into_raw(
    config: AudioConfig,
    stream_id: u32,
) -> Result<RawMessage, RtmpMessageSerializeError> {
    let payload = match config.codec {
        RtmpAudioCodec::Aac => FlvAudioData::Legacy(AudioTag {
            aac_packet_type: Some(AudioTagAacPacketType::Config),
            codec: LegacyFlvAudioCodec::Aac,
            sample_rate: AudioTagSoundRate::Rate44000,
            sample_size: AudioTagSampleSize::Sample16Bit,
            channels: config.channels,
            data: config.data,
        })
        .serialize()?,
        RtmpAudioCodec::Opus => FlvAudioData::Enhanced(ExAudioTag {
            four_cc: ExAudioFourCc::from(config.codec),
            packet: ExAudioPacket::SequenceStart(config.data),
            timestamp_offset_nanos: None,
        })
        .serialize()?,
    };

    Ok(RawMessage {
        msg_type: MessageType::Audio.into_raw(),
        stream_id,
        chunk_stream_id: AUDIO_CHUNK_STREAM_ID,
        timestamp: 0,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;

    use super::AudioMessage;
    use crate::{
        AudioChannels, AudioConfig, AudioData, ExAudioFourCc, ExAudioPacket, ExAudioTag,
        FlvAudioData, RtmpAudioCodec, TrackId,
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
    fn parses_enhanced_opus_coded_frames_as_data() {
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

        match message {
            AudioMessage::Data(data) => {
                assert_eq!(data.codec, RtmpAudioCodec::Opus);
                assert_eq!(data.pts.as_millis() as u32, 10);
                assert_eq!(data.data, Bytes::from_static(&[0xF0]));
            }
            other => panic!("expected Data, got {other:?}"),
        }
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
    fn serializes_legacy_audio_data_with_provided_channels() {
        let raw = AudioMessage::Data(AudioData {
            track_id: TrackId(1),
            codec: RtmpAudioCodec::Aac,
            pts: Duration::from_millis(33),
            data: Bytes::from_static(b"frame"),
        })
        .into_raw(1, AudioChannels::Mono)
        .unwrap();

        assert_eq!(raw.payload[0] & 0b0000_0001, 0);

        let raw = AudioMessage::Data(AudioData {
            track_id: TrackId(1),
            codec: RtmpAudioCodec::Aac,
            pts: Duration::from_millis(33),
            data: Bytes::from_static(b"frame"),
        })
        .into_raw(2, AudioChannels::Stereo)
        .unwrap();

        assert_eq!(raw.payload[0] & 0b0000_0001, 1);
    }

    // Opus ID header (RFC 7845 §5.1): "OpusHead" + version + channels + ...
    const OPUS_ID_HEADER_STEREO: &[u8] = &[
        b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', // magic
        1,    // version
        2,    // channel count = stereo
        0, 0, // pre-skip
        0x80, 0xBB, 0x00, 0x00, // sample rate 48000 LE
        0, 0, // output gain
        0, // mapping family
    ];

    const OPUS_ID_HEADER_MONO: &[u8] = &[
        b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', 1, 1, 0, 0, 0x80, 0xBB, 0x00, 0x00, 0, 0, 0,
    ];

    #[test]
    fn parses_enhanced_opus_sequence_start_stereo() {
        let payload = FlvAudioData::Enhanced(ExAudioTag {
            four_cc: ExAudioFourCc::Opus,
            packet: ExAudioPacket::SequenceStart(Bytes::from_static(OPUS_ID_HEADER_STEREO)),
            timestamp_offset_nanos: None,
        })
        .serialize()
        .unwrap();

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
                assert_eq!(config.codec, RtmpAudioCodec::Opus);
                assert_eq!(config.channels, AudioChannels::Stereo);
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn parses_enhanced_opus_sequence_start_mono() {
        let payload = FlvAudioData::Enhanced(ExAudioTag {
            four_cc: ExAudioFourCc::Opus,
            packet: ExAudioPacket::SequenceStart(Bytes::from_static(OPUS_ID_HEADER_MONO)),
            timestamp_offset_nanos: None,
        })
        .serialize()
        .unwrap();

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
                assert_eq!(config.codec, RtmpAudioCodec::Opus);
                assert_eq!(config.channels, AudioChannels::Mono);
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn parses_enhanced_opus_empty_sequence_start_defaults_to_stereo() {
        let payload = FlvAudioData::Enhanced(ExAudioTag {
            four_cc: ExAudioFourCc::Opus,
            packet: ExAudioPacket::SequenceStart(Bytes::from_static(&[])),
            timestamp_offset_nanos: None,
        })
        .serialize()
        .unwrap();

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
                assert_eq!(config.codec, RtmpAudioCodec::Opus);
                assert_eq!(config.channels, AudioChannels::Stereo);
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn serializes_enhanced_opus_data() {
        let raw = AudioMessage::Data(AudioData {
            track_id: TrackId::PRIMARY,
            codec: RtmpAudioCodec::Opus,
            pts: Duration::from_millis(100),
            data: Bytes::from_static(b"opus_frame"),
        })
        .into_raw(1, AudioChannels::Stereo)
        .unwrap();

        assert_eq!(raw.timestamp, 100);
        // First byte: soundFormat=9 (ExHeader), packetType=1 (CodedFrames)
        assert_eq!(raw.payload[0], 0x91);
        // Bytes 1..5: FourCC "Opus"
        assert_eq!(&raw.payload[1..5], b"Opus");
    }

    #[test]
    fn serializes_enhanced_opus_config() {
        let raw = AudioMessage::Config(AudioConfig {
            track_id: TrackId::PRIMARY,
            codec: RtmpAudioCodec::Opus,
            data: Bytes::from_static(OPUS_ID_HEADER_STEREO),
            channels: AudioChannels::Stereo,
        })
        .into_raw(1, AudioChannels::Stereo)
        .unwrap();

        assert_eq!(raw.timestamp, 0);
        // First byte: soundFormat=9 (ExHeader), packetType=0 (SequenceStart)
        assert_eq!(raw.payload[0], 0x90);
        assert_eq!(&raw.payload[1..5], b"Opus");
    }

    #[test]
    fn round_trip_enhanced_opus_data() {
        let original = AudioData {
            track_id: TrackId::PRIMARY,
            codec: RtmpAudioCodec::Opus,
            pts: Duration::from_millis(50),
            data: Bytes::from_static(b"opus_frame"),
        };

        let raw = AudioMessage::Data(original.clone())
            .into_raw(1, AudioChannels::Stereo)
            .unwrap();
        let parsed = AudioMessage::from_raw(raw).unwrap();

        match parsed {
            AudioMessage::Data(data) => {
                assert_eq!(data.codec, RtmpAudioCodec::Opus);
                assert_eq!(data.pts.as_millis(), 50);
                assert_eq!(data.data, Bytes::from_static(b"opus_frame"));
            }
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn serializes_enhanced_data_with_nano_offset() {
        let raw = AudioMessage::Data(AudioData {
            track_id: TrackId::PRIMARY,
            codec: RtmpAudioCodec::Opus,
            pts: Duration::from_nanos(100_000_777),
            data: Bytes::from_static(b"frame"),
        })
        .into_raw(1, AudioChannels::Stereo)
        .unwrap();

        let parsed = AudioMessage::from_raw(raw).unwrap();
        match parsed {
            AudioMessage::Data(data) => {
                assert_eq!(data.codec, RtmpAudioCodec::Opus);
                assert_eq!(data.pts.as_nanos(), 100_000_777);
            }
            other => panic!("expected Data, got {other:?}"),
        }
    }
}
