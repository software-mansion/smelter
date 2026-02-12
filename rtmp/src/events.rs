use std::time::Duration;

use bytes::Bytes;

use crate::{
    AudioChannels, AudioCodec, AudioTagSampleSize, AudioTagSoundRate, ScriptData, VideoCodec,
    VideoTagFrameType,
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
    pub channels: AudioChannels,
    pub sample_rate: u32,
    pub data: Bytes, // TODO: Audio specific config
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
        f.debug_struct("AacAudioConfig")
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
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
