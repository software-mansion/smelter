use bytes::{BufMut, Bytes, BytesMut};

use crate::{
    AudioCodecConversionError, RtmpAudioCodec, RtmpMessageSerializeError,
    error::FlvAudioTagParseError,
};

use super::{
    EX_AUDIO_SOUND_FORMAT,
    mod_ex_audio::{AudioPacketModExType, resolve_mod_ex, serialize_mod_ex},
};

// TODO: This is a struct while ExVideoTag is an enum. Rethink if audio might require multiple tag variants as well
/// Parsed Enhanced RTMP audio tag.
#[derive(Debug, Clone, PartialEq)]
pub struct ExAudioTag {
    pub four_cc: ExAudioFourCc,
    pub packet: ExAudioPacket,
    pub timestamp_offset_nanos: Option<u32>,
}

/// FourCC audio codec identifiers for Enhanced RTMP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExAudioFourCc {
    /// AC-3 (`ac-3`)
    Ac3,
    /// E-AC-3 (`ec-3`)
    Eac3,
    /// Opus (`Opus`)
    Opus,
    /// MP3 (`.mp3`)
    Mp3,
    /// FLAC (`fLaC`)
    Flac,
    /// AAC (`mp4a`)
    Aac,
}

impl ExAudioFourCc {
    fn from_raw(bytes: [u8; 4]) -> Result<Self, FlvAudioTagParseError> {
        match &bytes {
            b"ac-3" => Ok(Self::Ac3),
            b"ec-3" => Ok(Self::Eac3),
            b"Opus" => Ok(Self::Opus),
            b".mp3" => Ok(Self::Mp3),
            b"fLaC" => Ok(Self::Flac),
            b"mp4a" => Ok(Self::Aac),
            _ => Err(FlvAudioTagParseError::UnknownAudioFourCc(bytes)),
        }
    }

    fn into_raw(self) -> [u8; 4] {
        match self {
            Self::Ac3 => *b"ac-3",
            Self::Eac3 => *b"ec-3",
            Self::Opus => *b"Opus",
            Self::Mp3 => *b".mp3",
            Self::Flac => *b"fLaC",
            Self::Aac => *b"mp4a",
        }
    }
}

impl TryFrom<ExAudioFourCc> for RtmpAudioCodec {
    type Error = AudioCodecConversionError;

    fn try_from(four_cc: ExAudioFourCc) -> Result<Self, Self::Error> {
        match four_cc {
            ExAudioFourCc::Aac => Ok(RtmpAudioCodec::Aac),
            ExAudioFourCc::Ac3
            | ExAudioFourCc::Eac3
            | ExAudioFourCc::Opus
            | ExAudioFourCc::Mp3
            | ExAudioFourCc::Flac => {
                Err(AudioCodecConversionError::UnsupportedEnhancedFlv(four_cc))
            }
        }
    }
}

/// Semantic audio packet type after parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum ExAudioPacket {
    /// Decoder configuration / codec header packet.
    SequenceStart(Bytes),
    /// Codec frame payload.
    CodedFrames(Bytes),
    /// End of sequence marker. No payload.
    SequenceEnd,
    /// Multichannel channel mapping/config metadata.
    MultichannelConfig(Bytes),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ExAudioPacketType {
    SequenceStart,
    CodedFrames,
    SequenceEnd,
    MultichannelConfig,
    Multitrack,
    ModEx,
}

impl ExAudioPacketType {
    pub(super) fn from_raw(value: u8) -> Result<Self, FlvAudioTagParseError> {
        match value {
            0 => Ok(Self::SequenceStart),
            1 => Ok(Self::CodedFrames),
            2 => Ok(Self::SequenceEnd),
            4 => Ok(Self::MultichannelConfig),
            5 => Ok(Self::Multitrack),
            7 => Ok(Self::ModEx),
            _ => Err(FlvAudioTagParseError::UnknownExAudioPacketType(value)),
        }
    }

    pub(super) fn into_raw(self) -> u8 {
        match self {
            Self::SequenceStart => 0,
            Self::CodedFrames => 1,
            Self::SequenceEnd => 2,
            Self::MultichannelConfig => 4,
            Self::Multitrack => 5,
            Self::ModEx => 7,
        }
    }
}

impl ExAudioTag {
    /// Parses Enhanced RTMP audio tag.
    /// First byte: `[SoundFormat(4 bits) | AudioPacketType(4 bits)]`
    pub(super) fn parse(data: Bytes) -> Result<Self, FlvAudioTagParseError> {
        if data.is_empty() {
            return Err(FlvAudioTagParseError::TooShort);
        }

        let sound_format = (data[0] & 0b11110000) >> 4;
        if sound_format != EX_AUDIO_SOUND_FORMAT {
            return Err(FlvAudioTagParseError::UnknownCodecId(sound_format));
        }

        let packet_type = ExAudioPacketType::from_raw(data[0] & 0b00001111)?;

        // Process ModEx to resolve the final packet type and collect modifiers.
        let (packet_type, rest, timestamp_offset_nanos) = if packet_type == ExAudioPacketType::ModEx
        {
            let result = resolve_mod_ex(data.slice(1..))?;
            (
                result.packet_type,
                result.remaining,
                result.timestamp_offset_nanos,
            )
        } else {
            (packet_type, data.slice(1..), None)
        };

        if packet_type == ExAudioPacketType::Multitrack {
            return Err(FlvAudioTagParseError::UnsupportedPacketType(
                packet_type.into_raw(),
            ));
        }

        if rest.len() < 4 {
            return Err(FlvAudioTagParseError::TooShort);
        }

        let four_cc = ExAudioFourCc::from_raw([rest[0], rest[1], rest[2], rest[3]])?;
        let body_data = rest.slice(4..);

        let packet = match packet_type {
            ExAudioPacketType::SequenceStart => ExAudioPacket::SequenceStart(body_data),
            ExAudioPacketType::CodedFrames => ExAudioPacket::CodedFrames(body_data),
            ExAudioPacketType::SequenceEnd => ExAudioPacket::SequenceEnd,
            ExAudioPacketType::MultichannelConfig => ExAudioPacket::MultichannelConfig(body_data),
            ExAudioPacketType::Multitrack => unreachable!("Multitrack is handled above"),
            ExAudioPacketType::ModEx => unreachable!("ModEx should have been resolved above"),
        };

        Ok(ExAudioTag {
            four_cc,
            packet,
            timestamp_offset_nanos,
        })
    }

    pub(super) fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        let wire_packet_type = match &self.packet {
            ExAudioPacket::SequenceStart(_) => ExAudioPacketType::SequenceStart,
            ExAudioPacket::CodedFrames(_) => ExAudioPacketType::CodedFrames,
            ExAudioPacket::SequenceEnd => ExAudioPacketType::SequenceEnd,
            ExAudioPacket::MultichannelConfig(_) => ExAudioPacketType::MultichannelConfig,
        };

        let has_mod_ex = self.timestamp_offset_nanos.is_some();
        let header_packet_type = if has_mod_ex {
            ExAudioPacketType::ModEx
        } else {
            wire_packet_type
        };

        let first_byte = (EX_AUDIO_SOUND_FORMAT << 4) | header_packet_type.into_raw();

        let body_data = match &self.packet {
            ExAudioPacket::SequenceStart(data) => &data[..],
            ExAudioPacket::CodedFrames(data) => &data[..],
            ExAudioPacket::SequenceEnd => &[][..],
            ExAudioPacket::MultichannelConfig(data) => &data[..],
        };

        let mod_ex_data: Option<[u8; 3]> = match self.timestamp_offset_nanos {
            Some(nanos) if nanos > 999_999 => {
                return Err(RtmpMessageSerializeError::InternalError(format!(
                    "timestamp_offset_nanos {nanos} exceeds max 999999"
                )));
            }
            Some(nanos) => {
                let bytes = nanos.to_be_bytes();
                Some([bytes[1], bytes[2], bytes[3]])
            }
            None => None,
        };

        let mod_ex_size = mod_ex_data.as_ref().map_or(0, |data| data.len() + 2);
        let capacity = 1 + mod_ex_size + 4 + body_data.len();

        let mut buf = BytesMut::with_capacity(capacity);
        buf.put_u8(first_byte);

        if let Some(data) = &mod_ex_data {
            serialize_mod_ex(
                &mut buf,
                AudioPacketModExType::TimestampOffsetNano,
                data,
                wire_packet_type,
            )?;
        }

        buf.put(&self.four_cc.into_raw()[..]);
        buf.put(body_data);
        Ok(buf.freeze())
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{ExAudioFourCc, ExAudioPacket, ExAudioTag};
    use crate::error::FlvAudioTagParseError;

    #[test]
    fn parses_aac_sequence_start() {
        let data = Bytes::from_static(&[
            0x90, // [soundFormat=9 ExHeader | packetType=0 SequenceStart]
            b'm', b'p', b'4', b'a', 0x12, 0x10,
        ]);

        let parsed = ExAudioTag::parse(data).unwrap();
        assert_eq!(parsed.timestamp_offset_nanos, None);
        match parsed.packet {
            ExAudioPacket::SequenceStart(payload) => {
                assert_eq!(payload, Bytes::from_static(&[0x12, 0x10]));
            }
            other => panic!("unexpected parsed value: {other:?}"),
        }
    }

    #[test]
    fn parses_modex_timestamp_offset() {
        let data = Bytes::from_static(&[
            0x97, // [soundFormat=9 ExHeader | packetType=7 ModEx]
            2, 0x00, 0x00, 0x64, // ModEx data size=3, offset=100ns
            0x01, // [audioPacketModExType=0 TimestampOffsetNano | next packetType=1 CodedFrames]
            b'm', b'p', b'4', b'a', b'a', b'a', b'c',
        ]);

        let parsed = ExAudioTag::parse(data).unwrap();
        assert_eq!(parsed.timestamp_offset_nanos, Some(100));
        match parsed.packet {
            ExAudioPacket::CodedFrames(payload) => {
                assert_eq!(payload, Bytes::from_static(b"aac"));
            }
            other => panic!("unexpected parsed value: {other:?}"),
        }
    }

    #[test]
    fn rejects_multitrack_until_implemented() {
        let data = Bytes::from_static(&[
            0x95, // [soundFormat=9 ExHeader | packetType=5 Multitrack]
            b'm', b'p', b'4', b'a',
        ]);

        let err = ExAudioTag::parse(data).unwrap_err();
        assert!(matches!(
            err,
            FlvAudioTagParseError::UnsupportedPacketType(5)
        ));
    }

    #[test]
    fn round_trip_sequence_start() {
        let tag = ExAudioTag {
            four_cc: ExAudioFourCc::Aac,
            packet: ExAudioPacket::SequenceStart(Bytes::from_static(&[0x12, 0x10])),
            timestamp_offset_nanos: None,
        };
        let parsed = ExAudioTag::parse(tag.serialize().unwrap()).unwrap();
        assert_eq!(parsed, tag);
    }

    #[test]
    fn round_trip_coded_frames() {
        let tag = ExAudioTag {
            four_cc: ExAudioFourCc::Opus,
            packet: ExAudioPacket::CodedFrames(Bytes::from_static(b"opus_frame")),
            timestamp_offset_nanos: None,
        };
        let parsed = ExAudioTag::parse(tag.serialize().unwrap()).unwrap();
        assert_eq!(parsed, tag);
    }

    #[test]
    fn round_trip_sequence_end() {
        let tag = ExAudioTag {
            four_cc: ExAudioFourCc::Flac,
            packet: ExAudioPacket::SequenceEnd,
            timestamp_offset_nanos: None,
        };
        let parsed = ExAudioTag::parse(tag.serialize().unwrap()).unwrap();
        assert_eq!(parsed, tag);
    }

    #[test]
    fn round_trip_multichannel_config() {
        let tag = ExAudioTag {
            four_cc: ExAudioFourCc::Aac,
            packet: ExAudioPacket::MultichannelConfig(Bytes::from_static(&[0x00, 0x02])),
            timestamp_offset_nanos: None,
        };
        let parsed = ExAudioTag::parse(tag.serialize().unwrap()).unwrap();
        assert_eq!(parsed, tag);
    }

    #[test]
    fn round_trip_with_timestamp_offset() {
        let tag = ExAudioTag {
            four_cc: ExAudioFourCc::Aac,
            packet: ExAudioPacket::CodedFrames(Bytes::from_static(b"aac")),
            timestamp_offset_nanos: Some(999_999),
        };
        let parsed = ExAudioTag::parse(tag.serialize().unwrap()).unwrap();
        assert_eq!(parsed, tag);
    }

    #[test]
    fn round_trip_all_fourcc_variants() {
        for four_cc in [
            ExAudioFourCc::Ac3,
            ExAudioFourCc::Eac3,
            ExAudioFourCc::Opus,
            ExAudioFourCc::Mp3,
            ExAudioFourCc::Flac,
            ExAudioFourCc::Aac,
        ] {
            let tag = ExAudioTag {
                four_cc,
                packet: ExAudioPacket::CodedFrames(Bytes::from_static(b"data")),
                timestamp_offset_nanos: None,
            };
            let parsed = ExAudioTag::parse(tag.serialize().unwrap()).unwrap();
            assert_eq!(parsed, tag);
        }
    }

    #[test]
    fn serialize_rejects_timestamp_offset_above_max() {
        let tag = ExAudioTag {
            four_cc: ExAudioFourCc::Aac,
            packet: ExAudioPacket::CodedFrames(Bytes::from_static(b"data")),
            timestamp_offset_nanos: Some(1_000_000),
        };
        assert!(tag.serialize().is_err());
    }
}
