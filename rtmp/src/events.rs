use std::time::Duration;

use bytes::{Buf, Bytes};

use crate::{
    AudioChannels, AudioCodec, AudioSpecificConfigParseError, AudioTagSampleSize,
    AudioTagSoundRate, ParseError, ScriptData, VideoCodec, VideoTagFrameType,
};

#[derive(Debug, Clone)]
pub enum RtmpEvent {
    H264Data(H264VideoData),
    H264Config(H264VideoConfig),
    // H264EndOfSequence
    AacData(AacAudioData),
    AacConfig(AacAudioConfig),
    // Raw RTMP message for codecs that we do not explicitly support.
    GenericAudioData(GenericAudioData),
    // Raw RTMP message for codecs that we do not explicitly support.
    GenericVideoData(GenericVideoData),
    Metadata(ScriptData),
}

#[derive(Clone)]
pub struct AacAudioData {
    pub pts: Duration,
    pub data: Bytes,
    pub channels: AudioChannels,
}

#[derive(Clone)]
pub struct AacAudioConfig {
    data: Bytes, // TODO: Audio specific config
}

impl AacAudioConfig {
    pub fn new(data: Bytes) -> Self {
        Self { data }
    }

    pub fn data(&self) -> Bytes {
        self.data.clone()
    }

    pub fn sample_rate(&self) -> Result<u32, ParseError> {
        if self.data.remaining() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        let object_type = (self.data[0] >> 3) & 0x1F;
        let frequency_index = match object_type {
            31 => (self.data[1] >> 1) & 0xF,
            _ => {
                let high = self.data[0] & 0b111;
                let low = (self.data[1] >> 7) & 0b1;
                (high << 1) | low
            }
        };

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
            15 => match object_type {
                31 => {
                    let first_chunk = self.data[1] & 0x1;
                    let second_chunk = self.data[2];
                    let third_chunk = self.data[3];
                    let fourth_chunk = self.data[4] >> 1;

                    let first_byte = (first_chunk << 7) | (second_chunk >> 1);
                    let second_byte = (second_chunk << 7) | (third_chunk >> 1);
                    let third_byte = (third_chunk << 7) | fourth_chunk;

                    u32::from_be_bytes([0, first_byte, second_byte, third_byte])
                }
                _ => {
                    let first_chunk = self.data[1] >> 1;
                    let second_chunk = self.data[2];
                    let third_chunk = self.data[3];
                    let fourth_chunk = self.data[4] >> 7;

                    let first_byte = (first_chunk << 1) | (second_chunk >> 7);
                    let second_byte = (second_chunk << 1) | (third_chunk >> 7);
                    let third_byte = (third_chunk << 1) | fourth_chunk;

                    u32::from_be_bytes([0, first_byte, second_byte, third_byte])
                }
            },
            _ => {
                return Err(
                    AudioSpecificConfigParseError::InvalidFrequencyIndex(frequency_index).into(),
                );
            }
        };

        Ok(frequency)
    }

    pub fn channels(&self) -> Result<AudioChannels, ParseError> {
        Ok(AudioChannels::Stereo)
    }
}

// Raw RTMP message for codecs that we do not explicitly support.
#[derive(Clone)]
pub struct GenericAudioData {
    pub timestamp: u32,

    /// This value might not represent real sample rate for some codecs
    pub sound_rate: AudioTagSoundRate,
    // Only applies to PCM formats
    pub sample_size: Option<AudioTagSampleSize>,
    pub codec: AudioCodec,
    pub channels: AudioChannels,
    pub data: Bytes,
}

#[derive(Clone)]
pub struct H264VideoData {
    pub pts: Duration,
    pub dts: Duration,
    pub data: Bytes,
    pub is_keyframe: bool,
}

#[derive(Clone)]
pub struct H264VideoConfig {
    pub data: Bytes,
}

// Raw RTMP message for codecs that we do not explicitly support.
#[derive(Clone)]
pub struct GenericVideoData {
    pub timestamp: u32,

    /// This value might not represent real sample rate for some codecs
    pub codec: VideoCodec,
    pub frame_type: VideoTagFrameType,
    pub data: Bytes,
}

impl From<AacAudioConfig> for RtmpEvent {
    fn from(value: AacAudioConfig) -> Self {
        RtmpEvent::AacConfig(value)
    }
}

impl From<AacAudioData> for RtmpEvent {
    fn from(value: AacAudioData) -> Self {
        RtmpEvent::AacData(value)
    }
}

impl From<H264VideoConfig> for RtmpEvent {
    fn from(value: H264VideoConfig) -> Self {
        RtmpEvent::H264Config(value)
    }
}

impl From<H264VideoData> for RtmpEvent {
    fn from(value: H264VideoData) -> Self {
        RtmpEvent::H264Data(value)
    }
}

impl std::fmt::Debug for H264VideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H264VideoData")
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("data", &bytes_debug(&self.data))
            .field("is_keyframe", &self.is_keyframe)
            .finish()
    }
}

impl std::fmt::Debug for H264VideoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H264VideoConfig")
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for AacAudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AacAudioData")
            .field("pts", &self.pts)
            .field("data", &bytes_debug(&self.data))
            .field("channels", &self.channels)
            .finish()
    }
}

impl std::fmt::Debug for AacAudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sample_rate = self.sample_rate().map_err(|_| std::fmt::Error)?;
        let channels = self.channels().map_err(|_| std::fmt::Error)?;
        f.debug_struct("AacAudioConfig")
            .field("channels", &channels)
            .field("sample_rate", &sample_rate)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for GenericAudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenericAudioData")
            .field("timestamp", &self.timestamp)
            .field("sound_rate", &self.sound_rate)
            .field("sample_size", &self.sample_size)
            .field("codec", &self.codec)
            .field("channels", &self.channels)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for GenericVideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenericVideoData")
            .field("timestamp", &self.timestamp)
            .field("codec", &self.codec)
            .field("frame_type", &self.frame_type)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

fn bytes_debug(data: &[u8]) -> String {
    if data.len() <= 10 {
        format!("{data:?}")
    } else {
        format!(
            "({:?}, ..., {:?}), len={}",
            &data[..6],
            &data[(data.len() - 3)..],
            data.len()
        )
    }
}

#[cfg(test)]
mod asc_parser_test {
    use bytes::Bytes;

    use crate::AacAudioConfig;

    #[test]
    fn test_sound_frequency() {
        // Encoded with sample rate 44100 Hz
        let asc_bytes = Bytes::from_iter([0b00010_010, 0b0_0000000]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.sample_rate().unwrap(), 44_100);

        // Encoded with sample rate 48000 Hz
        let asc_bytes = Bytes::from_iter([0b00010_001, 0b1_0000000]);
        let asc = AacAudioConfig::new(asc_bytes);
        assert_eq!(asc.sample_rate().unwrap(), 48_000);
    }
}
