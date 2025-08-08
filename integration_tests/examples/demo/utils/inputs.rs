use std::fmt::Debug;

use anyhow::Result;
use rand::{thread_rng, RngCore};
use strum::{Display, EnumIter};

pub mod mp4;
pub mod rtp;
pub mod whip;

pub trait InputHandler: Debug {
    fn name(&self) -> &str;
    fn port(&self) -> u16;
    fn serialize(&self) -> serde_json::Value;
}

#[derive(Debug, EnumIter, Display, Clone)]
pub enum VideoSetupOptions {
    #[strum(to_string = "Decoder (default: ffmpeg_h264)")]
    Decoder,

    #[strum(to_string = "Done")]
    Done,
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

#[derive(Debug, EnumIter, Display, Clone)]
pub enum AudioSetupOptions {
    #[strum(to_string = "Decoder (default: opus)")]
    Decoder,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(Debug, Display, EnumIter)]
pub enum AudioDecoder {
    #[strum(to_string = "opus")]
    Opus,
    // #[strum(to_string = "aac")]
    // Aac(AacDecoderOptions),
}

pub fn input_name() -> String {
    let suffix = rand::thread_rng().next_u32().to_string();
    "input_".to_string() + suffix.as_str()
}
