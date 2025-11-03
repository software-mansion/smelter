use anyhow::Result;
use integration_tests::{ffmpeg, gstreamer};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use strum::{Display, EnumIter};

use crate::inputs::{
    hls::HlsInput, mp4::Mp4Input, rtp::RtpInput, whep::WhepInput, whip::WhipInput,
};

pub mod hls;
pub mod mp4;
pub mod rtp;
pub mod whep;
pub mod whip;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InputHandle {
    Rtp(RtpInput),
    Mp4(Mp4Input),
    Hls(HlsInput),
    Whip(WhipInput),
    Whep(WhepInput),
}

impl InputHandle {
    pub fn name(&self) -> &str {
        match self {
            Self::Rtp(i) => i.name(),
            Self::Mp4(i) => i.name(),
            Self::Hls(i) => i.name(),
            Self::Whip(i) => i.name(),
            Self::Whep(i) => i.name(),
        }
    }

    pub fn serialize_register(&self) -> serde_json::Value {
        match self {
            Self::Rtp(i) => i.serialize_register(),
            Self::Mp4(i) => i.serialize_register(),
            Self::Hls(i) => i.serialize_register(),
            Self::Whip(i) => i.serialize_register(),
            Self::Whep(i) => i.serialize_register(),
        }
    }

    pub fn has_video(&self) -> bool {
        match self {
            Self::Rtp(i) => i.has_video(),
            Self::Whep(i) => i.has_video(),
            Self::Whip(i) => i.has_video(),
            _ => true,
        }
    }

    pub fn has_audio(&self) -> bool {
        match self {
            Self::Rtp(i) => i.has_audio(),
            _ => true,
        }
    }

    pub fn on_before_registration(&mut self) -> Result<()> {
        match self {
            Self::Whep(i) => i.on_before_registration(),
            _ => Ok(()),
        }
    }

    pub fn on_after_registration(&mut self) -> Result<()> {
        match self {
            Self::Rtp(i) => i.on_after_registration(),
            Self::Whip(i) => i.on_after_registration(),
            _ => Ok(()),
        }
    }
}

impl std::fmt::Display for InputHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, EnumIter, Display, Clone, Copy)]
pub enum InputProtocol {
    #[strum(to_string = "rtp_stream")]
    Rtp,

    #[strum(to_string = "whip_server")]
    Whip,

    #[strum(to_string = "whep_client")]
    Whep,

    #[strum(to_string = "mp4")]
    Mp4,

    #[strum(to_string = "hls")]
    Hls,
}

#[derive(Debug, EnumIter, Display, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum VideoDecoder {
    #[strum(to_string = "any")]
    Any,

    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,

    #[strum(to_string = "vulkan_h264")]
    VulkanH264,

    #[strum(to_string = "ffmpeg_vp8")]
    FfmpegVp8,

    #[strum(to_string = "ffmpeg_vp9")]
    FfmpegVp9,
}

impl From<VideoDecoder> for gstreamer::Video {
    fn from(value: VideoDecoder) -> Self {
        match value {
            VideoDecoder::FfmpegH264 | VideoDecoder::VulkanH264 | VideoDecoder::Any => Self::H264,
            VideoDecoder::FfmpegVp8 => Self::VP8,
            VideoDecoder::FfmpegVp9 => Self::VP9,
        }
    }
}

impl From<VideoDecoder> for ffmpeg::Video {
    fn from(value: VideoDecoder) -> Self {
        match value {
            VideoDecoder::FfmpegH264 | VideoDecoder::VulkanH264 | VideoDecoder::Any => Self::H264,
            VideoDecoder::FfmpegVp8 => Self::VP8,
            VideoDecoder::FfmpegVp9 => Self::VP9,
        }
    }
}

#[derive(Debug, Display, EnumIter, Serialize, Deserialize, Clone, Copy)]
pub enum AudioDecoder {
    #[strum(to_string = "opus")]
    Opus,
}

pub fn filter_video_inputs(inputs: &[InputHandle]) -> Vec<&InputHandle> {
    inputs.iter().filter(|i| i.has_video()).collect()
}
