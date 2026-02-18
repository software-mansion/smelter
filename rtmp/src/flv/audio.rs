use bytes::{BufMut, Bytes, BytesMut};

use crate::{
    error::{AudioTagParseError, ParseError},
    SerializationError,
};

/// Struct representing flv AUDIODATA.
#[derive(Debug, Clone)]
pub struct AudioTag {
    /// SoundFormat 4bits
    pub codec: AudioCodec,
    /// SoundRate 2bits
    /// Represents sample rate in header, does not always mean it is a real value
    pub sample_rate: AudioTagSoundRate,
    /// SoundSize 1bit
    /// Size of the sample, only applies to PCM formats
    pub sample_size: AudioTagSampleSize,
    /// SoundType 1bit
    pub channels: AudioChannels,

    // AACPacketType 8bits IF SoundFormat == 10
    // AAC only
    pub aac_packet_type: Option<AudioTagAacPacketType>,

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
    // ExHeader (10) - for enhanced RTMP
    Aac,
    Speex,
    Mp3_8k,
    DeviceSpecific,
}

impl AudioCodec {
    fn from_raw(id: u8) -> Result<Self, AudioTagParseError> {
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
            _ => Err(AudioTagParseError::UnknownCodecId(id)),
        }
    }

    fn into_raw(self) -> u8 {
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
pub enum AudioTagSoundRate {
    Rate5500,
    Rate11000,
    Rate22000,
    Rate44000,
}

impl AudioTagSoundRate {
    /// value should be 2 bit value
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Rate5500,
            1 => Self::Rate11000,
            2 => Self::Rate22000,
            _ => Self::Rate44000,
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            Self::Rate5500 => 0,
            Self::Rate11000 => 1,
            Self::Rate22000 => 2,
            Self::Rate44000 => 3,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AudioTagSampleSize {
    Sample16Bit,
    Sample8Bit, // PCM only
}

impl AudioTagSampleSize {
    /// value should be 1 bit value
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Sample8Bit,
            _ => Self::Sample16Bit,
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            Self::Sample8Bit => 0,
            Self::Sample16Bit => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

impl AudioChannels {
    /// value should be 1 bit value
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Mono,
            _ => Self::Stereo,
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            Self::Mono => 0,
            Self::Stereo => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioTagAacPacketType {
    Data,
    Config,
}

impl AudioTagAacPacketType {
    fn from_raw(value: u8) -> Result<Self, AudioTagParseError> {
        match value {
            0 => Ok(Self::Config),
            1 => Ok(Self::Data),
            _ => Err(AudioTagParseError::InvalidAacPacketType(value)),
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            Self::Config => 0,
            Self::Data => 1,
        }
    }
}

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
        let sample_size = (data[0] & 0b00000010) >> 1;
        let sound_type = data[0] & 0b00000001;

        let codec = AudioCodec::from_raw(sound_format)?;
        let sample_rate = AudioTagSoundRate::from_raw(sample_rate);
        let sample_size = AudioTagSampleSize::from_raw(sample_size);
        let channels = AudioChannels::from_raw(sound_type);
        match codec {
            AudioCodec::Aac => Ok(Self::parse_aac(data, channels)?),
            _ => Ok(Self {
                aac_packet_type: None,
                codec,
                sample_rate,
                sample_size,
                channels,
                data,
            }),
        }
    }

    fn parse_aac(data: Bytes, channels: AudioChannels) -> Result<Self, ParseError> {
        if data.len() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        let aac_packet_type = AudioTagAacPacketType::from_raw(data[1])?;
        let audio_data = data.slice(2..);
        Ok(Self {
            codec: AudioCodec::Aac,
            sample_size: AudioTagSampleSize::Sample16Bit,
            sample_rate: AudioTagSoundRate::Rate44000,
            channels,
            aac_packet_type: Some(aac_packet_type),
            data: audio_data,
        })
    }

    pub fn serialize(&self) -> Result<Bytes, SerializationError> {
        let sound_format = self.codec.into_raw();
        let sound_rate = self.sample_rate.into_raw();
        let sample_size = self.sample_size.into_raw();
        let sound_type = self.channels.into_raw();

        // 4 bits format, 2 bits sound rate, 1 bit sample size, 1 bit sound type
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
        let Some(aac_packet_type) = self.aac_packet_type else {
            return Err(SerializationError::AacPacketTypeRequired);
        };
        data.put_u8(aac_packet_type.into_raw());
        data.put(&self.data[..]);
        Ok(data.freeze())
    }
}
