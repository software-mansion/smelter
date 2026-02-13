use std::time::Duration;

use bytes::Bytes;

use crate::{AudioChannels, AudioCodec, SampleSize, ScriptData, VideoCodec, VideoFrameType};

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

#[derive(Debug, Clone)]
pub struct AacAudioData {
    pub pts: Duration,
    pub data: Bytes,
    pub channels: AudioChannels,
}

#[derive(Debug, Clone)]
pub struct AacAudioConfig {
    pub channels: AudioChannels,
    pub sample_rate: u32,
    pub data: Bytes, // TODO: Audio specific config
}

// Raw RTMP message for codecs that we do not explicitly support.
#[derive(Debug, Clone)]
pub struct GenericAudioData {
    pub timestamp: u32,

    /// This value might not represent real sample rate for some codecs
    pub sample_rate: u32,
    // Only applies to PCM formats
    pub sample_size: Option<SampleSize>,
    pub codec: AudioCodec,
    pub channels: AudioChannels,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct H264VideoData {
    pub pts: Duration,
    pub dts: Duration,
    pub data: Bytes,
    pub is_keyframe: bool,
}

#[derive(Debug, Clone)]
pub struct H264VideoConfig {
    pub data: Bytes,
}

// Raw RTMP message for codecs that we do not explicitly support.
#[derive(Debug, Clone)]
pub struct GenericVideoData {
    pub timestamp: u32,

    /// This value might not represent real sample rate for some codecs
    pub codec: VideoCodec,
    pub frame_type: VideoFrameType,
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

