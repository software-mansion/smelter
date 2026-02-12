use bytes::Bytes;

use crate::{AudioChannels, AudioCodec, VideoCodec, VideoFrameType};

#[derive(Debug, Clone)]
pub struct AudioData {
    // TODO: switch to duration, it's not clear what time base those values are
    pub pts: i64,
    pub dts: i64,
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: AudioChannels,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: AudioChannels,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct VideoData {
    pub pts: i64,
    pub dts: i64,
    pub codec: VideoCodec,
    pub frame_type: VideoFrameType,
    pub composition_time: Option<i32>,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct VideoConfig {
    pub codec: VideoCodec,
    pub data: Bytes,
}
