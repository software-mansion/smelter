use std::fmt::Debug;

use anyhow::Result;
use serde_json::json;
use strum::{Display, EnumIter};

pub mod rtmp;
pub mod rtp;

pub trait OutputHandler: Debug {
    fn name(&self) -> &str;
    fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value;
    fn on_before_registration(&mut self) -> Result<()> {
        Ok(())
    }
    fn on_after_registration(&mut self) -> Result<()> {
        Ok(())
    }
}

impl std::fmt::Display for dyn OutputHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Display, EnumIter, Clone, Copy)]
pub enum OutputProtocol {
    #[strum(to_string = "rtp_stream")]
    Rtp,

    #[strum(to_string = "rtmp")]
    Rtmp,

    #[strum(to_string = "whip")]
    Whip,

    #[strum(to_string = "mp4")]
    Mp4,
}

#[derive(Debug, EnumIter, Display, Clone)]
pub enum VideoSetupOptions {
    #[strum(to_string = "Resolution (default: 1920x1080)")]
    Resolution,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(Debug)]
pub struct VideoResolution {
    pub width: u16,
    pub height: u16,
}

impl VideoResolution {
    pub fn serialize(&self) -> serde_json::Value {
        json!({
            "width": self.width,
            "height": self.height,
        })
    }
}

impl std::fmt::Display for VideoResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Debug, Display, EnumIter)]
pub enum VideoResolutionOptions {
    #[strum(to_string = "2560x1440")]
    Qhd,

    #[strum(to_string = "1920x1080")]
    Fhd,

    #[strum(to_string = "1280x720")]
    Hd,
}

#[derive(Debug, Display, EnumIter)]
pub enum VideoEncoder {
    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,

    #[strum(to_string = "ffmpeg_vp8")]
    FfmpegVp8,

    #[strum(to_string = "ffmpeg_vp9")]
    FfmpegVp9,
}

#[derive(Debug, Display, EnumIter)]
pub enum AudioEncoder {
    #[strum(to_string = "opus")]
    Opus,

    #[strum(to_string = "aac")]
    Aac,
}
