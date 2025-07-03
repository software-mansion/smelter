use std::io::Read;

use bytes::Buf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSpecificConfig {
    pub profile: u8,
    pub sample_rate: u32,
    pub channel_count: u8,
    pub frame_length: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioSpecificConfigParseError {
    #[error("ASC is not long enough")]
    TooShort,

    #[error("Illegal value in ASC")]
    IllegalValue,
}

impl AudioSpecificConfig {
    // MPEG-4 part 3, sections 1.6.2.1 & 4.4.1
    pub fn parse_from(data: &[u8]) -> Result<AudioSpecificConfig, AudioSpecificConfigParseError> {
        // TODO: this can probably be rewritten using [nom](https://lib.rs/crates/nom), which would
        // make it a lot more understandable
        let mut reader = std::io::Cursor::new(data);

        if reader.remaining() < 2 {
            return Err(AudioSpecificConfigParseError::TooShort);
        }

        let first = reader.get_u8();
        let second = reader.get_u8();

        let mut profile = (0b11111000 & first) >> 3;
        let sample_rate: u32;
        let channel_count: u8;
        let frame_length: u32;

        if profile == 31 {
            profile = ((first & 0b00000111) << 3) + ((second & 0b11100000) >> 5) + 0b00100000;
            let frequency_id = (second & 0b00011110) >> 1;

            let channel_and_frame_len_bytes: [u8; 2];

            if frequency_id == 15 {
                if reader.remaining() < 4 {
                    return Err(AudioSpecificConfigParseError::TooShort);
                }

                let mut rest = [0; 4];
                reader.read_exact(&mut rest).unwrap();

                sample_rate = (((second & 0b00000001) as u32) << 23)
                    | ((rest[0] as u32) << 15)
                    | ((rest[1] as u32) << 7)
                    | (((rest[2] & 0b11111110) >> 1) as u32);

                channel_and_frame_len_bytes = [rest[2], rest[3]];
            } else {
                if reader.remaining() < 1 {
                    return Err(AudioSpecificConfigParseError::TooShort);
                }
                let last = reader.get_u8();

                channel_and_frame_len_bytes = [second, last];
                sample_rate = freq_id_to_sample_rate(frequency_id)?
            };

            let [b1, b2] = channel_and_frame_len_bytes;
            channel_count = ((b1 & 0b00000001) << 3) | ((b2 & 0b11100000) >> 5);
            let frame_length_flag = b2 & 0b00010000 != 0;

            frame_length = frame_length_flag_to_frame_length(frame_length_flag);
        } else {
            let frequency_id = ((first & 0b00000111) << 1) + ((second & 0b10000000) >> 7);
            let channel_and_frame_len_byte: u8;

            if frequency_id == 15 {
                if reader.remaining() < 3 {
                    return Err(AudioSpecificConfigParseError::TooShort);
                }

                let mut rest = [0; 3];
                reader.read_exact(&mut rest).unwrap();
                sample_rate = (((second & 0b01111111) as u32) << 17)
                    | ((rest[0] as u32) << 9)
                    | ((rest[1] as u32) << 1)
                    | (((rest[2] & 0b10000000) >> 7) as u32);

                channel_and_frame_len_byte = rest[2];
            } else {
                sample_rate = freq_id_to_sample_rate(frequency_id)?;
                channel_and_frame_len_byte = second;
            }

            channel_count = (channel_and_frame_len_byte & 0b01111000) >> 3;
            let frame_length_flag = channel_and_frame_len_byte & 0b00000100 != 0;
            frame_length = frame_length_flag_to_frame_length(frame_length_flag);
        }

        Ok(AudioSpecificConfig {
            profile,
            sample_rate,
            channel_count,
            frame_length,
        })
    }
}

/// MPEG-4 part 3, 1.6.3.4
fn freq_id_to_sample_rate(id: u8) -> Result<u32, AudioSpecificConfigParseError> {
    match id {
        0x0 => Ok(96000),
        0x1 => Ok(88200),
        0x2 => Ok(64000),
        0x3 => Ok(48000),
        0x4 => Ok(44100),
        0x5 => Ok(32000),
        0x6 => Ok(24000),
        0x7 => Ok(22050),
        0x8 => Ok(16000),
        0x9 => Ok(12000),
        0xa => Ok(11025),
        0xb => Ok(8000),
        0xc => Ok(7350),
        _ => Err(AudioSpecificConfigParseError::IllegalValue),
    }
}

/// MPEG-4 part 3, 4.5.1.1
fn frame_length_flag_to_frame_length(flag: bool) -> u32 {
    match flag {
        true => 960,
        false => 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asc_simple() {
        let asc = [0b00010010, 0b00010000];
        let parsed = AudioSpecificConfig::parse_from(&asc).unwrap();

        assert_eq!(parsed.profile, 2);
        assert_eq!(parsed.sample_rate, 44_100);
        assert_eq!(parsed.channel_count, 2);
        assert_eq!(parsed.frame_length, 1024);
    }

    #[test]
    fn asc_complicated_frequency() {
        let asc = [0b00010111, 0b10000000, 0b00010000, 0b10011011, 0b10010100];
        let parsed = AudioSpecificConfig::parse_from(&asc).unwrap();

        assert_eq!(parsed.profile, 2);
        assert_eq!(parsed.sample_rate, 0x2137);
        assert_eq!(parsed.channel_count, 2);
        assert_eq!(parsed.frame_length, 960);
    }

    #[test]
    fn asc_complicated_profile() {
        let asc = [0b11111001, 0b01000110, 0b00100000];
        let parsed = AudioSpecificConfig::parse_from(&asc).unwrap();

        assert_eq!(parsed.profile, 42);
        assert_eq!(parsed.sample_rate, 48_000);
        assert_eq!(parsed.channel_count, 1);
        assert_eq!(parsed.frame_length, 1024);
    }

    #[test]
    fn asc_complicated_profile_and_frequency() {
        let asc = [
            0b11111001, 0b01011110, 0b00000000, 0b01000010, 0b01101110, 0b01000000,
        ];
        let parsed = AudioSpecificConfig::parse_from(&asc).unwrap();

        assert_eq!(parsed.profile, 42);
        assert_eq!(parsed.sample_rate, 0x2137);
        assert_eq!(parsed.channel_count, 2);
        assert_eq!(parsed.frame_length, 1024);
    }
}
