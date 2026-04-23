use std::{collections::HashMap, time::Duration};

use bytes::Bytes;

use crate::{AudioChannels, amf0::AmfValue};

mod aac;
pub use aac::AacAudioConfig;

/// Identifier for a logical track within an RTMP stream.
///
/// For single-track streams (present case) [`TrackId::PRIMARY`] is used. When
/// Enhanced RTMP multitrack parsing lands, non-primary ids will be populated
/// from the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackId(pub u8);

impl TrackId {
    pub const PRIMARY: Self = Self(0);
}

impl Default for TrackId {
    fn default() -> Self {
        Self::PRIMARY
    }
}

/// Public video codec identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RtmpVideoCodec {
    H264,
    Hevc,
    Vvc,
    Vp8,
    Vp9,
    Av1,
}

/// Public audio codec identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RtmpAudioCodec {
    Aac,
    Mp3 { sample_rate: u32, sample_size: u8 },
    Mp3_8k { sample_size: u8 },
    Pcm { sample_rate: u32, sample_size: u8 },
    Adpcm { sample_rate: u32, sample_size: u8 },
    PcmLe { sample_rate: u32, sample_size: u8 },
    Nellymoser { sample_rate: u32, sample_size: u8 },
    Nellymoser8kMono { sample_size: u8 },
    Nellymoser16kMono { sample_size: u8 },
    G711ALaw { sample_rate: u32, sample_size: u8 },
    G711MuLaw { sample_rate: u32, sample_size: u8 },
    Speex { sample_rate: u32, sample_size: u8 },
    DeviceSpecific { sample_rate: u32, sample_size: u8 },
}

#[derive(Debug, Clone)]
pub enum RtmpEvent {
    VideoData(VideoData),
    VideoConfig(VideoConfig),
    AudioData(AudioData),
    AudioConfig(AudioConfig),
    Metadata(HashMap<String, AmfValue>),
}

#[derive(Clone)]
pub struct VideoData {
    pub track_id: TrackId,
    pub codec: RtmpVideoCodec,
    pub pts: Duration,
    pub dts: Duration,
    pub data: Bytes,
    pub is_keyframe: bool,
}

#[derive(Clone)]
pub struct VideoConfig {
    pub track_id: TrackId,
    pub codec: RtmpVideoCodec,
    pub data: Bytes,
}

#[derive(Clone)]
pub struct AudioData {
    pub track_id: TrackId,
    pub codec: RtmpAudioCodec,
    pub pts: Duration,
    pub data: Bytes,
    pub channels: AudioChannels,
}

#[derive(Clone)]
pub struct AudioConfig {
    pub track_id: TrackId,
    pub codec: RtmpAudioCodec,
    pub data: Bytes,
}

impl From<VideoData> for RtmpEvent {
    fn from(value: VideoData) -> Self {
        RtmpEvent::VideoData(value)
    }
}

impl From<VideoConfig> for RtmpEvent {
    fn from(value: VideoConfig) -> Self {
        RtmpEvent::VideoConfig(value)
    }
}

impl From<AudioData> for RtmpEvent {
    fn from(value: AudioData) -> Self {
        RtmpEvent::AudioData(value)
    }
}

impl From<AudioConfig> for RtmpEvent {
    fn from(value: AudioConfig) -> Self {
        RtmpEvent::AudioConfig(value)
    }
}

impl std::fmt::Debug for VideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Video")
            .field("track_id", &self.track_id)
            .field("codec", &self.codec)
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("data", &bytes_debug(&self.data))
            .field("is_keyframe", &self.is_keyframe)
            .finish()
    }
}

impl std::fmt::Debug for VideoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoConfig")
            .field("track_id", &self.track_id)
            .field("codec", &self.codec)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for AudioData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("track_id", &self.track_id)
            .field("codec", &self.codec)
            .field("pts", &self.pts)
            .field("channels", &self.channels)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

impl std::fmt::Debug for AudioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioConfig")
            .field("track_id", &self.track_id)
            .field("codec", &self.codec)
            .field("data", &bytes_debug(&self.data))
            .finish()
    }
}

pub(crate) fn bytes_debug(data: &[u8]) -> String {
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
