use std::time::Duration;

use tracing::warn;

use crate::{
    AacAudioConfig, AudioChannels, AudioConfig, AudioData, AudioTag, AudioTagAacPacketType,
    AudioTagSampleSize, AudioTagSoundRate, ExAudioPacket, ExAudioTag, FlvAudioData,
    LegacyFlvAudioCodec, RtmpAudioCodec, RtmpMessageParseError, RtmpMessageSerializeError, TrackId,
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
            FlvAudioData::Legacy(tag) => Ok(Self::from_legacy(msg.timestamp, tag)),
            FlvAudioData::Enhanced(tag) => Ok(Self::from_enhanced(msg.timestamp, tag)),
        }
    }

    fn from_legacy(timestamp: u32, tag: AudioTag) -> Self {
        let codec = match RtmpAudioCodec::try_from(tag.codec) {
            Ok(codec) => codec,
            Err(err) => {
                warn!("{err}. Returning Unknown.");
                return Self::Unknown;
            }
        };

        let pts = Duration::from_millis(timestamp.into());

        match (codec, tag.aac_packet_type) {
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
        }
    }

    fn from_enhanced(timestamp: u32, tag: ExAudioTag) -> Self {
        let codec = match RtmpAudioCodec::try_from(tag.four_cc) {
            Ok(codec) => codec,
            Err(err) => {
                warn!("{err}. Returning Unknown.");
                return Self::Unknown;
            }
        };

        let nanos_offset = u64::from(tag.timestamp_offset_nanos.unwrap_or(0));
        let pts = Duration::from_millis(timestamp.into()) + Duration::from_nanos(nanos_offset);

        match tag.packet {
            ExAudioPacket::SequenceStart(data) => Self::Config(AudioConfig {
                track_id: TrackId::PRIMARY,
                codec,
                data,
            }),
            ExAudioPacket::CodedFrames(data) => Self::Data(AudioData {
                track_id: TrackId::PRIMARY,
                codec,
                pts,
                // ExAudio coded frames do not carry channel flags in the packet header.
                channels: AudioChannels::Stereo,
                data,
            }),
            ExAudioPacket::SequenceEnd | ExAudioPacket::MultichannelConfig(_) => Self::Unknown,
        }
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

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::AudioMessage;
    use crate::{
        AudioChannels, RtmpAudioCodec,
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
                assert_eq!(data.channels, AudioChannels::Stereo);
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
}
