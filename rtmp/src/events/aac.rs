use bytes::{Buf, Bytes};

use crate::{AudioChannels, AudioSpecificConfigParseError, ParseError};

#[derive(Clone)]
pub struct AacAudioConfig {
    data: Bytes,
}

impl AacAudioConfig {
    pub fn new(data: Bytes) -> Self {
        Self { data }
    }

    pub fn data(&self) -> &Bytes {
        &self.data
    }

    pub fn sample_rate(&self) -> Result<u32, ParseError> {
        let (object_type, frequency_index) = self.object_type_frequency_index()?;

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
                if self.data.remaining() < 5 {
                    return Err(ParseError::NotEnoughData);
                }
                match object_type {
                    31 => {
                        let first_chunk = self.data[1] & 0b0000_0001;
                        let second_chunk = self.data[2];
                        let third_chunk = self.data[3];
                        let fourth_chunk = (self.data[4] & 0b1111_1110) >> 1;

                        let first_byte = (first_chunk << 7) | (second_chunk >> 1);
                        let second_byte = (second_chunk << 7) | (third_chunk >> 1);
                        let third_byte = (third_chunk << 7) | fourth_chunk;

                        u32::from_be_bytes([0, first_byte, second_byte, third_byte])
                    }
                    _ => {
                        let first_chunk = self.data[1] & 0b0111_1111;
                        let second_chunk = self.data[2];
                        let third_chunk = self.data[3];
                        let fourth_chunk = (self.data[4] & 0b1000_0000) >> 7;

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

    pub fn channels(&self) -> Result<AudioChannels, ParseError> {
        let (object_type, frequency_index) = self.object_type_frequency_index()?;

        // 4 bit channel_configuration field
        let channel_configuration = match (object_type, frequency_index) {
            (31, 15) => {
                if self.data.remaining() < 6 {
                    return Err(ParseError::NotEnoughData);
                }
                let high = self.data[4] & 0b0000_0001;
                let low = (self.data[5] & 0b1110_0000) >> 5;

                (high << 3) | low
            }
            (31, _) => {
                if self.data.remaining() < 3 {
                    return Err(ParseError::NotEnoughData);
                }
                let high = self.data[1] & 0b0000_0001;
                let low = (self.data[2] & 0b1110_0000) >> 5;

                (high << 3) | low
            }
            (_, 15) => {
                if self.data.remaining() < 5 {
                    return Err(ParseError::NotEnoughData);
                }
                (self.data[4] & 0b0111_1000) >> 3
            }
            (_, _) => (self.data[1] & 0b0111_1000) >> 3,
        };

        match channel_configuration {
            1 => Ok(AudioChannels::Mono),
            2 => Ok(AudioChannels::Stereo),
            _ => Err(
                AudioSpecificConfigParseError::InvalidAudioChannel(channel_configuration).into(),
            ),
        }
    }

    fn object_type_frequency_index(&self) -> Result<(u8, u8), ParseError> {
        if self.data.remaining() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        // 5 bit object_type
        let object_type = (self.data[0] & 0b1111_1000) >> 3;

        // 4 bit frequency_index
        let frequency_index = match object_type {
            // If object_type == 31, then additional 6 bits come after initial 5 bits.
            31 => (self.data[1] & 0b0001_1110) >> 1,
            _ => {
                let high = self.data[0] & 0b0000_0111;
                let low = (self.data[1] & 0b1000_0000) >> 7;
                (high << 1) | low
            }
        };

        Ok((object_type, frequency_index))
    }
}

impl std::fmt::Debug for AacAudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sample_rate = self.sample_rate();
        let channels = self.channels();
        f.debug_struct("AacAudioConfig")
            .field("channels", &channels)
            .field("sample_rate", &sample_rate)
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
    fn test_sound_frequency() {
        // Encoded with sample rate 48000 Hz.
        // ASC format:
        // - 5 bits - object type
        // - 4 bits - frequency index
        let asc_bytes = Bytes::from_iter([0b0001_0001, 0b1000_0000]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.sample_rate().unwrap(), 48_000);

        // Encoded with sample rate 48000 Hz. object_type == 31
        // ASC format:
        // - 5 bits - object type
        // - 6 bits - object type extension
        // - 4 bits - frequency index
        let asc_bytes = Bytes::from_iter([0b1111_1000, 0b0000_0110]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.sample_rate().unwrap(), 48_000);

        // Encoded with custom sample rate (2137 Hz).
        // ASC format:
        // - 5 bits - object type
        // - 4 bits - frequency index
        // - 24 bits - custom frequency
        let asc_bytes = Bytes::from_iter([
            0b0001_0111,
            0b1000_0000,
            0b0000_0100,
            0b0010_1100,
            0b1000_0000,
        ]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.sample_rate().unwrap(), 2137);

        // Encoded with custom sample rate (2137 Hz). object_type == 31
        // ASC format:
        // - 5 bits - object type
        // - 6 bits - object type extension
        // - 4 bits - frequency index
        // - 24 bits - custom frequency
        let asc_bytes = Bytes::from_iter([
            0b1111_1000,
            0b0001_1110,
            0b0000_0000,
            0b0001_0000,
            0b1011_0010,
        ]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.sample_rate().unwrap(), 2137);
    }

    #[test]
    fn test_channels() {
        // Encoded with channels value 2.
        // ASC format:
        // - 5 bits - object type
        // - 4 bits - frequency index
        // - 4 bits - channel configuration
        let asc_bytes = Bytes::from_iter([0b0001_0001, 0b1001_0000]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.channels().unwrap(), AudioChannels::Stereo);

        // Encoded with channels value 2. object_type == 31
        // ASC format:
        // - 5 bits - object type
        // - 6 bits - object type extension
        // - 4 bits - frequency index
        // - 4 bits - channel configuration
        let asc_bytes = Bytes::from_iter([0b1111_1000, 0b0000_0110, 0b0100_0000]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.channels().unwrap(), AudioChannels::Stereo);

        // Encoded with channels value 2. frequency_index == 15
        // ASC format:
        // - 5 bits - object type
        // - 4 bits - frequency index
        // - 24 bits - custom frequency
        // - 4 bits - channel configuration
        let asc_bytes = Bytes::from_iter([
            0b0001_0111,
            0b1000_0000,
            0b0000_0000,
            0b0000_0000,
            0b0001_0000,
        ]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.channels().unwrap(), AudioChannels::Stereo);

        // Encoded with channels value 2. object_type == 31, frequency_index == 15
        // ASC format:
        // - 5 bits - object type
        // - 6 bits - object type extension
        // - 4 bits - frequency index
        // - 24 bits - custom frequency
        // - 4 bits - channel configuration
        let asc_bytes = Bytes::from_iter([
            0b1111_1000,
            0b0001_1110,
            0b0000_0000,
            0b0000_0000,
            0b0000_0000,
            0b0100_0000,
        ]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.channels().unwrap(), AudioChannels::Stereo);
    }
}
