use anyhow::Result;
use enum_iterator::Sequence;
use std::fmt::Display;

pub mod mp4;
pub mod rtp;
pub mod whip;

pub trait InputHandler {
    fn name(&self) -> &str;
    fn setup_video(&mut self) -> Result<()>;
    fn setup_audio(&mut self) -> Result<()>;
}

#[derive(Sequence)]
pub enum VideoSetupOptions {
    Decoder,
    Done,
}

impl Display for VideoSetupOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Decoder => "Decoder",
            Self::Done => "Done",
        };
        write!(f, "{msg}")
    }
}

#[derive(Sequence)]
pub enum VideoDecoder {
    FfmpegH264,
    FfmpegVp8,
    FfmpegVp9,
}

impl Display for VideoDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::FfmpegH264 => "ffmpeg_h264",
            Self::FfmpegVp8 => "ffmpeg_vp8",
            Self::FfmpegVp9 => "ffmpeg_vp9",
        };
        write!(f, "{msg}")
    }
}

#[derive(Sequence)]
pub enum AudioSetupOptions {
    Decoder,
    Done,
}

impl Display for AudioSetupOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Decoder => "Decoder",
            Self::Done => "Done",
        };
        write!(f, "{msg}")
    }
}

#[derive(Sequence)]
pub enum AudioDecoder {
    Opus,
    Aac,
}

impl Display for AudioDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Opus => "opus",
            Self::Aac => "aac",
        };
        write!(f, "{msg}")
    }
}
