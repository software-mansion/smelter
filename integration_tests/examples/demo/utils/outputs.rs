use std::fmt::Debug;

use strum::{Display, EnumIter};

pub mod rtp;

pub trait OutputHandle: Debug {
    fn name(&self) -> &str;
    fn port(&self) -> u16;
    fn serialize(&self) -> serde_json::Value;
}

#[derive(Debug, Display, EnumIter)]
pub enum OutputProtocol {
    #[strum(to_string = "rtp")]
    Rtp,
}

#[derive(Debug, EnumIter, Display, Clone)]
pub enum VideoSetupOptions {
    #[strum(to_string = "Resolution (default: 1920x1080)")]
    Resolution,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(Debug, Display, EnumIter)]
pub enum VideoEncoder {
    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,
}
