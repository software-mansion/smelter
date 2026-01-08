use bytes::Bytes;
use thiserror::Error;

use crate::{AudioChannels, AudioCodec, AudioTag, error::ParseError, tag::PacketType};

impl TryFrom<u8> for AudioCodec {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use crate::AudioCodec::*;
        match value {
            0 => Ok(Pcm),
            1 => Ok(Adpcm),
            2 => Ok(Mp3),
            3 => Ok(PcmLe),
            4 => Ok(Nellymoser16kMono),
            5 => Ok(Nellymoser8kMono),
            6 => Ok(Nellymoser),
            7 => Ok(G711ALaw),
            8 => Ok(G711MuLaw),
            10 => Ok(Aac),
            11 => Ok(Speex),
            14 => Ok(Mp3_8k),
            15 => Ok(DeviceSpecific),
            _ => Err(ParseError::UnsupportedCodec(value)),
        }
    }
}

impl From<AudioCodec> for u8 {
    fn from(value: AudioCodec) -> Self {
        use crate::AudioCodec::*;
        match value {
            Pcm => 0,
            Adpcm => 1,
            Mp3 => 2,
            PcmLe => 3,
            Nellymoser16kMono => 4,
            Nellymoser8kMono => 5,
            Nellymoser => 6,
            G711ALaw => 7,
            G711MuLaw => 8,
            Aac => 10,
            Speex => 11,
            Mp3_8k => 14,
            DeviceSpecific => 15,
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AudioTagParseError {
    #[error("Invalid sound rate header value: {0}")]
    InvalidSoundRate(u8),

    #[error("Invalid sound type header value: {0}")]
    InvalidSoundType(u8),

    #[error("Invalid AacPacketType header value: {0}")]
    InvalidAacPacketType(u8),
}

impl AudioTag {
    // Currently only AAC audio codec is supported
    pub(super) fn parse(data: Bytes) -> Result<Self, ParseError> {
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

        let codec = AudioCodec::try_from(sound_format)?;
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
        Err(ParseError::UnsupportedCodec(codec.into()))
    }
}
