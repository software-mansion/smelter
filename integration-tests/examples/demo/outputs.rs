use std::fmt::Debug;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter};

use crate::{
    inputs::InputHandle,
    outputs::{
        hls::HlsOutput, mp4::Mp4Output, rtmp::RtmpOutput, rtp::RtpOutput, whep::WhepOutput,
        whip::WhipOutput,
    },
};

pub mod hls;
pub mod mp4;
pub mod rtmp;
pub mod rtp;
pub mod whep;
pub mod whip;

pub mod scene;

#[derive(Debug, Serialize, Deserialize)]
pub enum OutputHandle {
    Rtp(RtpOutput),
    Rtmp(RtmpOutput),
    Mp4(Mp4Output),
    Whip(WhipOutput),
    Whep(WhepOutput),
    Hls(HlsOutput),
}

impl OutputHandle {
    pub fn name(&self) -> &str {
        todo!()
    }

    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        todo!()
    }
    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
        todo!()
    }

    pub fn on_before_registration(&mut self) -> Result<()> {
        match self {
            _ => Ok(()),
        }
    }

    pub fn on_after_registration(&mut self) -> Result<()> {
        match self {
            _ => Ok(()),
        }
    }
}

impl std::fmt::Display for OutputHandle {
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

#[derive(Debug, Display, EnumIter, PartialEq, Serialize, Deserialize, Clone, Copy)]
pub enum VideoEncoder {
    #[strum(to_string = "any")]
    Any,

    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,

    #[strum(to_string = "ffmpeg_vp8")]
    FfmpegVp8,

    #[strum(to_string = "ffmpeg_vp9")]
    FfmpegVp9,
}

#[derive(Debug, Display, EnumIter, PartialEq, Serialize, Deserialize, Clone, Copy)]
pub enum AudioEncoder {
    #[strum(to_string = "any")]
    Any,

    #[strum(to_string = "opus")]
    Opus,

    #[strum(to_string = "aac")]
    Aac,
}
