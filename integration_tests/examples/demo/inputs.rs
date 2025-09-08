use anyhow::Result;
use integration_tests::{ffmpeg, gstreamer};
use std::fmt::Debug;
use strum::{Display, EnumIter};

use crate::players::InputPlayer;

pub mod mp4;
pub mod rtp;
pub mod whip;

pub trait InputHandler: Debug {
    fn name(&self) -> &str;
    fn has_video(&self) -> bool {
        true
    }

    fn has_audio(&self) -> bool {
        true
    }

    fn on_after_registration(&mut self, _player: InputPlayer) -> Result<()> {
        Ok(())
    }
}

impl std::fmt::Display for dyn InputHandler {
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
}

#[derive(Debug, EnumIter, Display, PartialEq, Clone, Copy)]
pub enum VideoDecoder {
    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,

    #[strum(to_string = "ffmpeg_vp8")]
    FfmpegVp8,

    #[strum(to_string = "ffmpeg_vp9")]
    FfmpegVp9,

    #[strum(to_string = "any")]
    Any,
}

impl From<VideoDecoder> for gstreamer::Video {
    fn from(value: VideoDecoder) -> Self {
        match value {
            VideoDecoder::FfmpegH264 | VideoDecoder::Any => Self::H264,
            VideoDecoder::FfmpegVp8 => Self::VP8,
            VideoDecoder::FfmpegVp9 => Self::VP9,
        }
    }
}

impl From<VideoDecoder> for ffmpeg::Video {
    fn from(value: VideoDecoder) -> Self {
        match value {
            VideoDecoder::FfmpegH264 | VideoDecoder::Any => Self::H264,
            VideoDecoder::FfmpegVp8 => Self::VP8,
            VideoDecoder::FfmpegVp9 => Self::VP9,
        }
    }
}

#[derive(Debug, Display, EnumIter)]
pub enum AudioDecoder {
    #[strum(to_string = "opus")]
    Opus,
    // TODO: AAC
}

pub fn filter_video_inputs<'a>(inputs: &'a [&'a dyn InputHandler]) -> Vec<&'a dyn InputHandler> {
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
