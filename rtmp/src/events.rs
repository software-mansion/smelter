use std::{collections::HashMap, time::Duration};

use bytes::Bytes;

use crate::{
    AudioChannels, AudioCodec, AudioTagSampleSize, AudioTagSoundRate, VideoTag, amf0::AmfValue,
    flv::ExVideoTag,
};

mod aac;
pub use aac::AacAudioConfig;

#[derive(Debug, Clone)]
pub enum RtmpEvent {
    H264Data(H264VideoData),
    /// H264 decoder config
    H264Config(H264VideoConfig),
    /// Raw legacy FLV video tag that is not mapped to a specialized event.
    LegacyVideoData(LegacyVideoData),
    /// Parsed Enhanced RTMP video tag payload.
    EnhancedVideoData(EnhancedVideoData),

    AacData(AacAudioData),
    /// AAC decoder config
    AacConfig(AacAudioConfig),
    /// Raw RTMP message for codecs that we do not explicitly support.
    UnknownAudioData(GenericAudioData),

    Metadata(HashMap<String, AmfValue>),
}

#[derive(Clone)]
pub struct AacAudioData {
    pub pts: Duration,
    pub data: Bytes,
    pub channels: AudioChannels,
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

// Raw legacy FLV video tag that is not mapped to a specialized event.
#[derive(Clone)]
pub struct LegacyVideoData {
    pub timestamp: u32,
    pub tag: VideoTag,
}

#[derive(Clone)]
pub struct EnhancedVideoData {
    pub timestamp: u32,
    pub tag: ExVideoTag,
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

impl From<LegacyVideoData> for RtmpEvent {
    fn from(value: LegacyVideoData) -> Self {
        RtmpEvent::LegacyVideoData(value)
    }
}

impl From<EnhancedVideoData> for RtmpEvent {
    fn from(value: EnhancedVideoData) -> Self {
        RtmpEvent::EnhancedVideoData(value)
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

impl std::fmt::Debug for LegacyVideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LegacyVideoData")
            .field("timestamp", &self.timestamp)
            .field("tag", &self.tag)
            .finish()
    }
}

impl std::fmt::Debug for EnhancedVideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnhancedVideoData")
            .field("timestamp", &self.timestamp)
            .field("tag", &self.tag)
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
