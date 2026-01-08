use bytes::Bytes;

use crate::{
    error::{AudioTagParseError, ParseError},
    tag::PacketType,
};

/// Struct representing flv AUDIODATA.
#[derive(Debug, Clone)]
pub struct AudioTag {
    pub packet_type: PacketType,
    pub codec: AudioCodec,
    pub sound_rate: u32,
    pub sound_type: AudioChannels,
    pub data: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCodec {
    Pcm,
    Adpcm,
    Mp3,
    PcmLe,
    Nellymoser16kMono,
    Nellymoser8kMono,
    Nellymoser,
    G711ALaw,
    G711MuLaw,
    Aac,
    Speex,
    Mp3_8k,
    DeviceSpecific,
}

impl AudioCodec {
    fn try_from_id(id: u8) -> Result<Self, ParseError> {
        match id {
            0 => Ok(Self::Pcm),
            1 => Ok(Self::Adpcm),
            2 => Ok(Self::Mp3),
            3 => Ok(Self::PcmLe),
            4 => Ok(Self::Nellymoser16kMono),
            5 => Ok(Self::Nellymoser8kMono),
            6 => Ok(Self::Nellymoser),
            7 => Ok(Self::G711ALaw),
            8 => Ok(Self::G711MuLaw),
            10 => Ok(Self::Aac),
            11 => Ok(Self::Speex),
            14 => Ok(Self::Mp3_8k),
            15 => Ok(Self::DeviceSpecific),
            _ => Err(ParseError::UnsupportedCodec(id)),
        }
    }

    fn into_id(self) -> u8 {
        match self {
            Self::Pcm => 0,
            Self::Adpcm => 1,
            Self::Mp3 => 2,
            Self::PcmLe => 3,
            Self::Nellymoser16kMono => 4,
            Self::Nellymoser8kMono => 5,
            Self::Nellymoser => 6,
            Self::G711ALaw => 7,
            Self::G711MuLaw => 8,
            Self::Aac => 10,
            Self::Speex => 11,
            Self::Mp3_8k => 14,
            Self::DeviceSpecific => 15,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

impl AudioTag {
    // Currently only AAC audio codec is supported
    pub(crate) fn parse(data: Bytes) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::NotEnoughData);
        }

        let sound_format = (data[0] >> 4) & 0x0F;
        let sound_rate = (data[0] >> 2) & 0x03;
        let sound_type = data[0] & 0x01;

        let channels = match sound_type {
            0 => AudioChannels::Mono,
            1 => AudioChannels::Stereo,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidSoundType(
                    sound_type,
                )));
            }
        };

        let sound_rate = match sound_rate {
            0 => 5500,
            1 => 11_000,
            2 => 22_050,
            3 => 44_100,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidSoundRate(
                    sound_rate,
                )));
            }
        };

        let codec = AudioCodec::try_from_id(sound_format)?;
        match codec {
            AudioCodec::Aac => Self::parse_aac(data, sound_rate, channels),
            _ => Self::parse_codec(data, codec, sound_rate, channels),
        }
    }

    fn parse_aac(
        mut data: Bytes,
        sound_rate: u32,
        channels: AudioChannels,
    ) -> Result<Self, ParseError> {
        if data.len() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        let aac_packet_type = data[1];

        let packet_type = match aac_packet_type {
            0 => PacketType::Config,
            1 => PacketType::Data,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidAacPacketType(
                    aac_packet_type,
                )));
            }
        };

        let audio_data = data.split_off(2);
        Ok(Self {
            packet_type,
            codec: AudioCodec::Aac,
            sound_rate,
            sound_type: channels,
            data: audio_data,
        })
    }

    // This function will be implemented when support for more audio codecs is added
    fn parse_codec(
        _data: Bytes,
        codec: AudioCodec,
        _sound_rate: u32,
        _channels: AudioChannels,
    ) -> Result<Self, ParseError> {
        Err(ParseError::UnsupportedCodec(codec.into_id()))
    }
}
