use bytes::{Buf, Bytes};

use crate::{AudioChannels, AudioSpecificConfigParseError, ParseError};

#[derive(Clone)]
pub struct AacAudioConfig {
    data: Bytes,
    channels: AudioChannels,
    sample_rate: u32,
}

impl TryFrom<Bytes> for AacAudioConfig {
    type Error = ParseError;

    fn try_from(data: Bytes) -> Result<Self, Self::Error> {
        let (object_type, frequency_index) = Self::parse_object_type_frequency_index(&data)?;
        let sample_rate = Self::parse_sample_rate(&data, object_type, frequency_index)?;
        let channels = Self::parse_channels(&data, object_type, frequency_index)?;

        Ok(Self {
            data,
            channels,
            sample_rate,
        })
    }
}

impl AacAudioConfig {
    pub fn data(&self) -> &Bytes {
        &self.data
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> AudioChannels {
        self.channels
    }

    fn parse_sample_rate(
        data: &Bytes,
        object_type: u8,
        frequency_index: u8,
    ) -> Result<u32, ParseError> {
        let frequency: u32 = match frequency_index {
            0 => 96000,
            1 => 88200,
            2 => 64000,
            3 => 48000,
            4 => 44100,
            5 => 32000,
            6 => 24000,
            7 => 22050,
            8 => 16000,
            9 => 12000,
            10 => 11025,
            11 => 8000,
            12 => 7350,

            // If frequency_index == 15, then the frequency is encoded explicitly as 24 bit number
            15 => {
                if data.remaining() < 5 {
                    return Err(ParseError::NotEnoughData);
                }
                match object_type {
                    31 => {
                        let first_chunk = data[1] & 0b0000_0001;
                        let second_chunk = data[2];
                        let third_chunk = data[3];
                        let fourth_chunk = (data[4] & 0b1111_1110) >> 1;

                        let first_byte = (first_chunk << 7) | (second_chunk >> 1);
                        let second_byte = (second_chunk << 7) | (third_chunk >> 1);
                        let third_byte = (third_chunk << 7) | fourth_chunk;

                        u32::from_be_bytes([0, first_byte, second_byte, third_byte])
                    }
                    _ => {
                        let first_chunk = data[1] & 0b0111_1111;
                        let second_chunk = data[2];
                        let third_chunk = data[3];
                        let fourth_chunk = (data[4] & 0b1000_0000) >> 7;

                        let first_byte = (first_chunk << 1) | (second_chunk >> 7);
                        let second_byte = (second_chunk << 1) | (third_chunk >> 7);
                        let third_byte = (third_chunk << 1) | fourth_chunk;

                        u32::from_be_bytes([0, first_byte, second_byte, third_byte])
                    }
                }
            }
            _ => {
                return Err(
                    AudioSpecificConfigParseError::InvalidFrequencyIndex(frequency_index).into(),
                );
            }
        };

        Ok(frequency)
    }

    fn parse_channels(
        data: &Bytes,
        object_type: u8,
        frequency_index: u8,
    ) -> Result<AudioChannels, ParseError> {
        // 4 bit channel_configuration field
        let channel_configuration = match (object_type, frequency_index) {
            (31, 15) => {
                if data.remaining() < 6 {
                    return Err(ParseError::NotEnoughData);
                }
                let high = data[4] & 0b0000_0001;
                let low = (data[5] & 0b1110_0000) >> 5;

                (high << 3) | low
            }
            (31, _) => {
                if data.remaining() < 3 {
                    return Err(ParseError::NotEnoughData);
                }
                let high = data[1] & 0b0000_0001;
                let low = (data[2] & 0b1110_0000) >> 5;

                (high << 3) | low
            }
            (_, 15) => {
                if data.remaining() < 5 {
                    return Err(ParseError::NotEnoughData);
                }
                (data[4] & 0b0111_1000) >> 3
            }
            (_, _) => (data[1] & 0b0111_1000) >> 3,
        };

        match channel_configuration {
            1 => Ok(AudioChannels::Mono),
            2 => Ok(AudioChannels::Stereo),
            _ => Err(
                AudioSpecificConfigParseError::InvalidAudioChannel(channel_configuration).into(),
            ),
        }
    }

    fn parse_object_type_frequency_index(data: &Bytes) -> Result<(u8, u8), ParseError> {
        if data.remaining() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        // 5 bit object_type
        let object_type = (data[0] & 0b1111_1000) >> 3;

        // 4 bit frequency_index
        let frequency_index = match object_type {
            // If object_type == 31, then additional 6 bits come after initial 5 bits.
            31 => (data[1] & 0b0001_1110) >> 1,
            _ => {
                let high = data[0] & 0b0000_0111;
                let low = (data[1] & 0b1000_0000) >> 7;
                (high << 1) | low
            }
        };

        Ok((object_type, frequency_index))
    }
}

impl std::fmt::Debug for AacAudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AacAudioConfig")
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .field("data", &crate::events::bytes_debug(&self.data))
            .finish()
    }
}

#[cfg(test)]
mod asc_parser_test {
    // ASC formatting:
    // https://wiki.multimedia.cx/index.php/MPEG-4_Audio#Audio_Specific_Config

    use bytes::Bytes;

    use crate::{AacAudioConfig, AudioChannels};

    #[test]
    fn test_asc_parsing() {
        // Encoded with sample rate 48000 Hz, Stereo.
        // ASC format:
        // - 5 bits - object type (2)
        // - 4 bits - frequency index (3)
        // - 4 bits - channel configuration (2)
        let asc_bytes = Bytes::from_iter([0b0001_0001, 0b1001_0000]);
        let asc = AacAudioConfig::try_from(asc_bytes).unwrap();
        assert_eq!(asc.sample_rate(), 48_000);
        assert_eq!(asc.channels(), AudioChannels::Stereo);

        // Encoded with sample rate 48000 Hz, Stereo.
        // ASC format:
        // - 5 bits - object type (31)
        // - 6 bits - object type extension (0)
        // - 4 bits - frequency index (3)
        // - 4 bits - channel configuration (2)
        let asc_bytes = Bytes::from_iter([0b1111_1000, 0b0000_0110, 0b0100_0000]);
        let asc = AacAudioConfig::try_from(asc_bytes).unwrap();
        assert_eq!(asc.sample_rate(), 48_000);
        assert_eq!(asc.channels(), AudioChannels::Stereo);

        // Encoded with custom sample rate (2137 Hz), Stereo.
        // ASC format:
        // - 5 bits - object type (2)
        // - 4 bits - frequency index (15)
        // - 24 bits - custom frequency (2137)
        // - 4 bits - channel configuration (2)
        let asc_bytes = Bytes::from_iter([
            0b0001_0111,
            0b1000_0000,
            0b0000_0100,
            0b0010_1100,
            0b1001_0000,
        ]);
        let asc = AacAudioConfig::try_from(asc_bytes).unwrap();
        assert_eq!(asc.sample_rate(), 2137);
        assert_eq!(asc.channels(), AudioChannels::Stereo);

        // Encoded with custom sample rate (2137 Hz), Stereo.
        // ASC format:
        // - 5 bits - object type (31)
        // - 6 bits - object type extension (0)
        // - 4 bits - frequency index (15)
        // - 24 bits - custom frequency (2137)
        // - 4 bits - channel configuration (2)
        let asc_bytes = Bytes::from_iter([
            0b1111_1000,
            0b0001_1110,
            0b0000_0000,
            0b0001_0000,
            0b1011_0010,
            0b0100_0000,
        ]);
        let asc = AacAudioConfig::try_from(asc_bytes).unwrap();
        assert_eq!(asc.sample_rate(), 2137);
        assert_eq!(asc.channels(), AudioChannels::Stereo);
    }
}
