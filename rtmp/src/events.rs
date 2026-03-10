use std::time::Duration;

use bytes::Bytes;

use crate::{
    AudioChannels, AudioCodec, AudioFourCc, AudioTagSampleSize, AudioTagSoundRate, ScriptData,
    VideoCodec, VideoFourCc, VideoTagFrameType,
};

mod aac;
pub use aac::AacAudioConfig;

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

    // Enhanced RTMP events
    /// Video frame data received via Enhanced RTMP (HEVC, AV1, VP9, or AVC via FourCC path).
    EnhancedVideoData(EnhancedVideoData),
    /// Video decoder configuration received via Enhanced RTMP (SequenceStart).
    EnhancedVideoConfig(EnhancedVideoConfig),
    /// Audio frame data received via Enhanced RTMP (Opus, FLAC, AC-3, or AAC via FourCC path).
    EnhancedAudioData(EnhancedAudioData),
    /// Audio decoder configuration received via Enhanced RTMP (SequenceStart).
    EnhancedAudioConfig(EnhancedAudioConfig),
}

impl RtmpEvent {
    pub fn is_media_packet(&self) -> bool {
        !matches!(
            self,
            RtmpEvent::H264Config(_)
                | RtmpEvent::AacConfig(_)
                | RtmpEvent::EnhancedVideoConfig(_)
                | RtmpEvent::EnhancedAudioConfig(_)
                | RtmpEvent::Metadata(_)
        )
    }
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

// Raw RTMP message for codecs that we do not explicitly support.
#[derive(Clone)]
pub struct GenericVideoData {
    pub timestamp: u32,

    /// This value might not represent real sample rate for some codecs
    pub codec: VideoCodec,
    pub frame_type: VideoTagFrameType,
    pub data: Bytes,
}

/// Video frame data received via Enhanced RTMP.
#[derive(Clone)]
pub struct EnhancedVideoData {
    pub fourcc: VideoFourCc,
    pub pts: Duration,
    pub dts: Duration,
    pub data: Bytes,
    pub is_keyframe: bool,
}

/// Video decoder configuration received via Enhanced RTMP (SequenceStart).
#[derive(Clone)]
pub struct EnhancedVideoConfig {
    pub fourcc: VideoFourCc,
    pub data: Bytes,
}

/// Audio frame data received via Enhanced RTMP.
#[derive(Clone)]
pub struct EnhancedAudioData {
    pub fourcc: AudioFourCc,
    pub pts: Duration,
    pub data: Bytes,
}

/// Audio decoder configuration received via Enhanced RTMP (SequenceStart).
#[derive(Clone)]
pub struct EnhancedAudioConfig {
    pub fourcc: AudioFourCc,
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

impl From<EnhancedVideoData> for RtmpEvent {
    fn from(value: EnhancedVideoData) -> Self {
        RtmpEvent::EnhancedVideoData(value)
    }
}

impl From<EnhancedVideoConfig> for RtmpEvent {
    fn from(value: EnhancedVideoConfig) -> Self {
        RtmpEvent::EnhancedVideoConfig(value)
    }
}

impl From<EnhancedAudioData> for RtmpEvent {
    fn from(value: EnhancedAudioData) -> Self {
        RtmpEvent::EnhancedAudioData(value)
    }
}

impl From<EnhancedAudioConfig> for RtmpEvent {
    fn from(value: EnhancedAudioConfig) -> Self {
        RtmpEvent::EnhancedAudioConfig(value)
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

impl std::fmt::Debug for EnhancedVideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnhancedVideoData")
            .field("fourcc", &self.fourcc)
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("data", &bytes_debug(&self.data))
            .field("is_keyframe", &self.is_keyframe)
            .finish()
    }
}

impl std::fmt::Debug for EnhancedVideoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnhancedVideoConfig")
            .field("fourcc", &self.fourcc)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for EnhancedAudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnhancedAudioData")
            .field("fourcc", &self.fourcc)
            .field("pts", &self.pts)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for EnhancedAudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnhancedAudioConfig")
            .field("fourcc", &self.fourcc)
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
