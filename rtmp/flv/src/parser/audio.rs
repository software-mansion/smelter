use bytes::Bytes;
use thiserror::Error;

use crate::{AudioChannels, AudioCodec, AudioTag, PacketType, ParseError};

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
    pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
        if payload.len() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        let sound_format = (payload[0] >> 4) & 0x0F;
        let sound_rate_opt = (payload[0] >> 2) & 0x03;
        let sound_type = payload[0] & 0x01;

        let codec = AudioCodec::try_from(sound_format)?;
        // NOTE: This should be removed when support for more codecs is added
        if codec != AudioCodec::Aac {
            return Err(ParseError::UnsupportedCodec(sound_format));
        }

        let aac_packet_type = payload[1];

        let sound_rate = match sound_rate_opt {
            0 => 5500,
            1 => 11_000,
            2 => 22_050,
            3 => 44_100,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidSoundRate(
                    sound_rate_opt,
                )));
            }
        };

        let channels = match sound_type {
            0 => AudioChannels::Mono,
            1 => AudioChannels::Stereo,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidSoundType(
                    sound_type,
                )));
            }
        };

        let packet_type = match aac_packet_type {
            0 => PacketType::AudioConfig,
            1 => PacketType::Audio,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidAacPacketType(
                    aac_packet_type,
                )));
            }
        };

        // This is true only for AAC, for any other codec it should be payload[1..]
        // Other codecs are not supported at the current time
        let audio_data = Bytes::copy_from_slice(&payload[2..]);
        Ok(Self {
            packet_type,
            codec,
            sound_rate,
            sound_type: channels,
            payload: audio_data,
        })
    }
}
