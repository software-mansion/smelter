use std::time::Duration;

use bytes::Bytes;

use crate::{
    AudioChannels, AudioCodec, AudioTagSampleSize, AudioTagSoundRate, ScriptData, VideoCodec,
    VideoTagFrameType,
};

mod aac;
pub use aac::AacAudioConfig;

#[derive(Debug, Clone)]
pub enum RtmpEvent {
    // Video
    H264Data(H264VideoData),
    H264Config(H264VideoConfig),
    HevcData(HevcVideoData),
    HevcConfig(HevcVideoConfig),
    Av1Data(Av1VideoData),
    Av1Config(Av1VideoConfig),
    Vp9Data(Vp9VideoData),
    Vp9Config(Vp9VideoConfig),

    // Audio
    AacData(AacAudioData),
    AacConfig(AacAudioConfig),
    OpusData(OpusAudioData),
    OpusConfig(OpusAudioConfig),
    FlacData(FlacAudioData),
    FlacConfig(FlacAudioConfig),
    Mp3Data(Mp3AudioData),
    Mp3Config(Mp3AudioConfig),
    Ac3Data(Ac3AudioData),
    Ac3Config(Ac3AudioConfig),
    Eac3Data(Eac3AudioData),
    Eac3Config(Eac3AudioConfig),

    // Raw RTMP message for codecs that we do not explicitly support.
    GenericAudioData(GenericAudioData),
    // Raw RTMP message for codecs that we do not explicitly support.
    GenericVideoData(GenericVideoData),

    Metadata(ScriptData),
}

impl RtmpEvent {
    pub fn is_media_packet(&self) -> bool {
        !matches!(
            self,
            RtmpEvent::H264Config(_)
                | RtmpEvent::HevcConfig(_)
                | RtmpEvent::Av1Config(_)
                | RtmpEvent::Vp9Config(_)
                | RtmpEvent::AacConfig(_)
                | RtmpEvent::OpusConfig(_)
                | RtmpEvent::FlacConfig(_)
                | RtmpEvent::Mp3Config(_)
                | RtmpEvent::Ac3Config(_)
                | RtmpEvent::Eac3Config(_)
                | RtmpEvent::Metadata(_)
        )
    }
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

#[derive(Clone)]
pub struct HevcVideoData {
    pub pts: Duration,
    pub dts: Duration,
    pub data: Bytes,
    pub is_keyframe: bool,
}

#[derive(Clone)]
pub struct HevcVideoConfig {
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Av1VideoData {
    pub pts: Duration,
    pub dts: Duration,
    pub data: Bytes,
    pub is_keyframe: bool,
}

#[derive(Clone)]
pub struct Av1VideoConfig {
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Vp9VideoData {
    pub pts: Duration,
    pub dts: Duration,
    pub data: Bytes,
    pub is_keyframe: bool,
}

#[derive(Clone)]
pub struct Vp9VideoConfig {
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

#[derive(Clone)]
pub struct AacAudioData {
    pub pts: Duration,
    pub data: Bytes,
    pub channels: AudioChannels,
}

#[derive(Clone)]
pub struct OpusAudioData {
    pub pts: Duration,
    pub data: Bytes,
}

#[derive(Clone)]
pub struct OpusAudioConfig {
    pub data: Bytes,
}

#[derive(Clone)]
pub struct FlacAudioData {
    pub pts: Duration,
    pub data: Bytes,
}

#[derive(Clone)]
pub struct FlacAudioConfig {
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Mp3AudioData {
    pub pts: Duration,
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Mp3AudioConfig {
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Ac3AudioData {
    pub pts: Duration,
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Ac3AudioConfig {
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Eac3AudioData {
    pub pts: Duration,
    pub data: Bytes,
}

#[derive(Clone)]
pub struct Eac3AudioConfig {
    pub data: Bytes,
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

impl From<HevcVideoData> for RtmpEvent {
    fn from(value: HevcVideoData) -> Self {
        RtmpEvent::HevcData(value)
    }
}

impl From<HevcVideoConfig> for RtmpEvent {
    fn from(value: HevcVideoConfig) -> Self {
        RtmpEvent::HevcConfig(value)
    }
}

impl From<Av1VideoData> for RtmpEvent {
    fn from(value: Av1VideoData) -> Self {
        RtmpEvent::Av1Data(value)
    }
}

impl From<Av1VideoConfig> for RtmpEvent {
    fn from(value: Av1VideoConfig) -> Self {
        RtmpEvent::Av1Config(value)
    }
}

impl From<Vp9VideoData> for RtmpEvent {
    fn from(value: Vp9VideoData) -> Self {
        RtmpEvent::Vp9Data(value)
    }
}

impl From<Vp9VideoConfig> for RtmpEvent {
    fn from(value: Vp9VideoConfig) -> Self {
        RtmpEvent::Vp9Config(value)
    }
}

impl From<OpusAudioData> for RtmpEvent {
    fn from(value: OpusAudioData) -> Self {
        RtmpEvent::OpusData(value)
    }
}

impl From<OpusAudioConfig> for RtmpEvent {
    fn from(value: OpusAudioConfig) -> Self {
        RtmpEvent::OpusConfig(value)
    }
}

impl From<FlacAudioData> for RtmpEvent {
    fn from(value: FlacAudioData) -> Self {
        RtmpEvent::FlacData(value)
    }
}

impl From<FlacAudioConfig> for RtmpEvent {
    fn from(value: FlacAudioConfig) -> Self {
        RtmpEvent::FlacConfig(value)
    }
}

impl From<Mp3AudioData> for RtmpEvent {
    fn from(value: Mp3AudioData) -> Self {
        RtmpEvent::Mp3Data(value)
    }
}

impl From<Mp3AudioConfig> for RtmpEvent {
    fn from(value: Mp3AudioConfig) -> Self {
        RtmpEvent::Mp3Config(value)
    }
}

impl From<Ac3AudioData> for RtmpEvent {
    fn from(value: Ac3AudioData) -> Self {
        RtmpEvent::Ac3Data(value)
    }
}

impl From<Ac3AudioConfig> for RtmpEvent {
    fn from(value: Ac3AudioConfig) -> Self {
        RtmpEvent::Ac3Config(value)
    }
}

impl From<Eac3AudioData> for RtmpEvent {
    fn from(value: Eac3AudioData) -> Self {
        RtmpEvent::Eac3Data(value)
    }
}

impl From<Eac3AudioConfig> for RtmpEvent {
    fn from(value: Eac3AudioConfig) -> Self {
        RtmpEvent::Eac3Config(value)
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

impl std::fmt::Debug for HevcVideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HevcVideoData")
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("data", &bytes_debug(&self.data))
            .field("is_keyframe", &self.is_keyframe)
            .finish()
    }
}

impl std::fmt::Debug for HevcVideoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HevcVideoConfig")
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Av1VideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Av1VideoData")
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("data", &bytes_debug(&self.data))
            .field("is_keyframe", &self.is_keyframe)
            .finish()
    }
}

impl std::fmt::Debug for Av1VideoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Av1VideoConfig")
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Vp9VideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vp9VideoData")
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("data", &bytes_debug(&self.data))
            .field("is_keyframe", &self.is_keyframe)
            .finish()
    }
}

impl std::fmt::Debug for Vp9VideoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vp9VideoConfig")
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

impl std::fmt::Debug for OpusAudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpusAudioData")
            .field("pts", &self.pts)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for OpusAudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpusAudioConfig")
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for FlacAudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlacAudioData")
            .field("pts", &self.pts)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for FlacAudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlacAudioConfig")
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Mp3AudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mp3AudioData")
            .field("pts", &self.pts)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Mp3AudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mp3AudioConfig")
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Ac3AudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ac3AudioData")
            .field("pts", &self.pts)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Ac3AudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ac3AudioConfig")
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Eac3AudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Eac3AudioData")
            .field("pts", &self.pts)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for Eac3AudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Eac3AudioConfig")
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
