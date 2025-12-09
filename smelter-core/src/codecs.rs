use smelter_render::Resolution;

mod aac;
mod h264;
mod opus;
mod vp8;
mod vp9;

pub use aac::*;
pub use h264::*;
pub use opus::*;
pub use vp8::*;
pub use vp9::*;

use crate::AudioChannels;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    H264,
    Vp8,
    Vp9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Aac,
    Opus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoDecoderOptions {
    FfmpegH264,
    FfmpegVp8,
    FfmpegVp9,
    VulkanH264,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDecoderOptions {
    Opus,
    FdkAac(FdkAacDecoderOptions),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum VideoEncoderOptions {
    FfmpegH264(FfmpegH264EncoderOptions),
    FfmpegVp8(FfmpegVp8EncoderOptions),
    FfmpegVp9(FfmpegVp9EncoderOptions),
    VulkanH264(VulkanH264EncoderOptions),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoEncoderBitrate {
    pub average_bitrate: u64,
    pub max_bitrate: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum AudioEncoderOptions {
    Opus(OpusEncoderOptions),
    FdkAac(FdkAacEncoderOptions),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputPixelFormat {
    YUV420P,
    YUV422P,
    YUV444P,
}

pub(crate) trait AudioEncoderOptionsExt {
    fn sample_rate(&self) -> u32;
}

impl VideoEncoderOptions {
    pub fn resolution(&self) -> Resolution {
        match self {
            VideoEncoderOptions::FfmpegH264(opt) => opt.resolution,
            VideoEncoderOptions::FfmpegVp8(opt) => opt.resolution,
            VideoEncoderOptions::FfmpegVp9(opt) => opt.resolution,
            VideoEncoderOptions::VulkanH264(opt) => opt.resolution,
        }
    }
}

impl AudioEncoderOptions {
    pub fn channels(&self) -> AudioChannels {
        match self {
            AudioEncoderOptions::Opus(options) => options.channels,
            AudioEncoderOptions::FdkAac(options) => options.channels,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        match self {
            AudioEncoderOptions::Opus(options) => options.sample_rate,
            AudioEncoderOptions::FdkAac(options) => options.sample_rate,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
    #[error(transparent)]
    OpusError(#[from] LibOpusDecoderError),
    #[error(transparent)]
    AacDecoder(#[from] FdkAacDecoderError),
}
