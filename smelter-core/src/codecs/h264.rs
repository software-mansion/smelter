use std::{sync::Arc, time::Duration};

use smelter_render::Resolution;

use crate::codecs::{OutputPixelFormat, VideoEncoderBitrate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FfmpegH264EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
    Placebo,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FfmpegH264EncoderOptions {
    pub preset: FfmpegH264EncoderPreset,
    pub bitrate: Option<VideoEncoderBitrate>,
    pub keyframe_interval: Duration,
    pub resolution: Resolution,
    pub pixel_format: OutputPixelFormat,
    pub raw_options: Vec<(Arc<str>, Arc<str>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VulkanH264EncoderOptions {
    pub resolution: Resolution,
    pub bitrate: Option<VulkanH264EncoderRateControl>,
    pub keyframe_interval: Duration,
    pub preset: VulkanH264EncoderPreset,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VulkanH264EncoderRateControl {
    VariableBitrate(VideoEncoderBitrate),
    ConstantBitrate(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VulkanH264EncoderPreset {
    HighQuality,
    LowLatency,
}

#[derive(Debug, thiserror::Error)]
pub enum H264AvcDecoderConfigError {
    #[error("Incorrect AVCDecoderConfig. Expected more bytes.")]
    NotEnoughBytes(#[from] bytes::TryGetError),

    #[error("Not AVCC")]
    NotAVCC,
}
