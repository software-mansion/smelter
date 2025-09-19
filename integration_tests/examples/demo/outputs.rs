use std::fmt::Debug;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter};

use crate::inputs::InputHandle;

pub mod hls;
pub mod mp4;
pub mod rtmp;
pub mod rtp;
pub mod whep;
pub mod whip;

pub mod scene;

#[typetag::serde(tag = "type")]
pub trait OutputHandle: Debug {
    fn name(&self) -> &str;
    fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value;
    fn serialize_update(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value;

    fn on_before_registration(&mut self) -> Result<()> {
        Ok(())
    }

    fn on_after_registration(&mut self) -> Result<()> {
        Ok(())
    }
}

impl std::fmt::Display for dyn OutputHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Display, EnumIter, Clone, Copy)]
pub enum OutputProtocol {
    #[strum(to_string = "rtp_stream")]
    Rtp,

    #[strum(to_string = "rtmp_client")]
    Rtmp,

    #[strum(to_string = "whip_client")]
    Whip,

    #[strum(to_string = "whep_server")]
    Whep,

    #[strum(to_string = "mp4")]
    Mp4,

    #[strum(to_string = "hls")]
    Hls,
}

#[allow(dead_code)]
#[derive(Debug, EnumIter, Display, Clone)]
pub enum VideoSetupOptions {
    #[strum(to_string = "Resolution (default: 1920x1080)")]
    Resolution,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[allow(dead_code)]
#[derive(Debug, Display, EnumIter)]
pub enum VideoResolutionOptions {
    #[strum(to_string = "2560x1440")]
    Qhd,

    #[strum(to_string = "1920x1080")]
    Fhd,

    #[strum(to_string = "1280x720")]
    Hd,
}

#[derive(Debug, Display, EnumIter, PartialEq, Serialize, Deserialize, Clone, Copy)]
pub enum VideoEncoder {
    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,

    #[strum(to_string = "ffmpeg_vp8")]
    FfmpegVp8,

    #[strum(to_string = "ffmpeg_vp9")]
    FfmpegVp9,

    #[strum(to_string = "any")]
    Any,
}

#[derive(Debug, Display, EnumIter, Serialize, Deserialize, Clone, Copy)]
pub enum AudioEncoder {
    #[strum(to_string = "opus")]
    Opus,

    #[strum(to_string = "aac")]
    Aac,

    #[strum(to_string = "any")]
    Any,
}
