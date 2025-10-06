use anyhow::Result;
use integration_tests::{ffmpeg, gstreamer};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use strum::{Display, EnumIter};

pub mod hls;
pub mod mp4;
pub mod rtp;
pub mod whip;

#[typetag::serde(tag = "type")]
pub trait InputHandle: Debug {
    fn name(&self) -> &str;
    fn serialize_register(&self) -> serde_json::Value;

    fn has_video(&self) -> bool {
        true
    }

    fn has_audio(&self) -> bool {
        true
    }

    fn on_after_registration(&mut self) -> Result<()> {
        Ok(())
    }
}

impl std::fmt::Display for dyn InputHandle {
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
    // TODO: AAC
}

pub fn filter_video_inputs<'a>(inputs: &'a [&'a dyn InputHandle]) -> Vec<&'a dyn InputHandle> {
    inputs
        .iter()
        .filter_map(|input| {
            if input.has_video() {
                Some(*input)
            } else {
                None
            }
        })
        .collect()
}
