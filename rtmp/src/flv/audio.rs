use bytes::{BufMut, Bytes, BytesMut};

use crate::{
    PacketType, SerializationError,
    error::{AudioTagParseError, ParseError},
};

/// Struct representing flv AUDIODATA.
#[derive(Debug, Clone)]
pub struct AudioTag {
    // AAC only
    pub packet_type: Option<PacketType>,

    pub codec: AudioCodec,
    /// represetnts sample rate in FLV header
    pub sample_rate: u32,
    pub sample_size: SampleSize,
    pub channels: AudioChannels,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub enum SampleSize {
    Sample16Bit,
    Sample8Bit, // PCM only
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
    // ExHeader (10) - for enhanced RTMP
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
            _ => Err(AudioTagParseError::UnknownCodecId(id).into()),
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

// Currently only AAC audio codec is supported
impl AudioTag {
    /// Parses flv `AUDIODATA`. The `data` must be the entire content of the `Data` field of
    /// the flv tag with audio `TagType`.  
    /// Check <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74> for more info.
    pub fn parse(data: Bytes) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::NotEnoughData);
        }

        let sound_format = (data[0] & 0b11110000) >> 4;
        let sample_rate = (data[0] & 0b00001100) >> 2;
        // assume 16
        let sample_size = (data[0] & 0b00000010) >> 1;
        let sound_type = data[0] & 0b00000001;

        let channels = match sound_type {
            0 => AudioChannels::Mono,
            1 => AudioChannels::Stereo,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidSoundType(
                    sound_type,
                )));
            }
        };
        let sample_rate = match sample_rate {
            0 => 5500,
            1 => 11_000,
            2 => 22_050,
            3 => 44_100,
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidSoundRate(
                    sample_rate,
                )));
            }
        };
        let sample_size = match sample_size {
            0 => SampleSize::Sample8Bit,
            _ => SampleSize::Sample16Bit,
        };

        let codec = AudioCodec::try_from_id(sound_format)?;
        match codec {
            AudioCodec::Aac => Ok(Self::parse_aac(data, sample_rate, sample_size, channels)?),
            _ => Ok(Self {
                packet_type: None,
                codec,
                sample_rate,
                sample_size,
                channels,
                data,
            }),
        }
    }

    fn parse_aac(
        data: Bytes,
        sample_rate: u32,
        sample_size: SampleSize,
        channels: AudioChannels,
    ) -> Result<Self, ParseError> {
        if data.len() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        let aac_packet_type = data[1];
        let packet_type = match aac_packet_type {
            0 => Some(PacketType::Config),
            1 => Some(PacketType::Data),
            _ => {
                return Err(ParseError::Audio(AudioTagParseError::InvalidAacPacketType(
                    aac_packet_type,
                )));
            }
        };

        let audio_data = data.slice(2..);
        Ok(Self {
            packet_type,
            codec: AudioCodec::Aac,
            sample_size,
            sample_rate,
            channels,
            data: audio_data,
        })
    }

    pub fn serialize(&self) -> Result<Bytes, SerializationError> {
        let sound_type: u8 = match self.channels {
            AudioChannels::Mono => 0,
            AudioChannels::Stereo => 1,
        };
        let sound_rate = match self.sample_rate {
            5500 => 0,
            11_000 => 1,
            22_050 => 2,
            44_100 => 3,
            _ => 3,
        };
        let sample_size = 1; // 1 - 16bit, 0 - 8bit
        let sound_format: u8 = AudioCodec::Aac.into_id();

        // 4 bits format, 2 bits sound rate, 1 bit sample size, 1 bity sound type
        let first_byte = (sound_format << 4) | (sound_rate << 2) | (sample_size << 1) | sound_type;
        match self.codec {
            AudioCodec::Aac => Ok(self.serialize_aac(first_byte)?),
            _ => {
                let mut data = BytesMut::with_capacity(self.data.len() + 1);
                data.put_u8(first_byte);
                data.put(&self.data[..]);
                Ok(data.freeze())
            }
        }
    }

    fn serialize_aac(&self, first_byte: u8) -> Result<Bytes, SerializationError> {
        let mut data = BytesMut::with_capacity(self.data.len() + 2);
        data.put_u8(first_byte);
        match self.packet_type {
            Some(PacketType::Data) => data.put_u8(1),
            Some(PacketType::Config) => data.put_u8(0),
            None => return Err(SerializationError::AacPacketTypeRequired),
        }
        data.put(&self.data[..]);
        Ok(data.freeze())
    }
}
