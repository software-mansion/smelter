use compositor_render::Resolution;

use crate::codecs::OutputPixelFormat;

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
    pub resolution: Resolution,
    pub pixel_format: OutputPixelFormat,
    pub raw_options: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VulkanH264EncoderRateControl {
    EncoderDefault,
    Vbr {
        average_bitrate: u64,
        max_bitrate: u64,
    },
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VulkanH264EncoderQualityLevel {
    Low,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VulkanH264EncoderOptions {
    pub resolution: Resolution,
    pub rate_control: VulkanH264EncoderRateControl,
    pub quality_level: VulkanH264EncoderQualityLevel,
}

#[derive(Debug, thiserror::Error)]
pub enum H264AvcDecoderConfigError {
    #[error("Incorrect AVCDecoderConfig. Expected more bytes.")]
    NotEnoughBytes(#[from] bytes::TryGetError),

    #[error("Not AVCC")]
    NotAVCC,
}
