use anyhow::Result;
use rand::RngCore;
use std::fmt::Debug;
use strum::{Display, EnumIter};

pub mod mp4;
pub mod rtp;
pub mod whip;

pub trait InputHandler: Debug {
    fn name(&self) -> &str;
    fn port(&self) -> u16;
    fn serialize(&self) -> serde_json::Value;
    fn start_ffmpeg_transmitter(&self) -> Result<()>;
}

impl std::fmt::Display for dyn InputHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}, port: {}", self.name(), self.port())
    }
}

#[derive(Debug, EnumIter, Display, Clone, Copy)]
pub enum InputProtocol {
    #[strum(to_string = "rtp_stream")]
    Rtp,

    #[strum(to_string = "whip")]
    Whip,

    #[strum(to_string = "mp4")]
    Mp4,
}

#[derive(Debug, EnumIter, Display)]
pub enum VideoDecoder {
    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,

    #[strum(to_string = "ffmpeg_vp8")]
    FfmpegVp8,

    #[strum(to_string = "ffmpeg_vp9")]
    FfmpegVp9,
}

#[derive(Debug, Display, EnumIter)]
pub enum AudioDecoder {
    #[strum(to_string = "opus")]
    Opus,
    // TODO: AAC
}

pub fn input_name() -> String {
    let suffix = rand::thread_rng().next_u32().to_string();
    format!("input_{suffix}")
}
