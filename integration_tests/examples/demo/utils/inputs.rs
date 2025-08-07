use anyhow::Result;
use strum::{Display, EnumIter};

pub mod mp4;
pub mod rtp;
pub mod whip;

pub trait InputHandler {
    fn name(&self) -> &str;
    fn setup_video(&mut self) -> Result<()>;
    fn setup_audio(&mut self) -> Result<()>;
}

#[derive(EnumIter, Display)]
pub enum VideoSetupOptions {
    #[strum(to_string = "Decoder")]
    Decoder,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(EnumIter, Display)]
pub enum VideoDecoder {
    #[strum(to_string = "ffmpeg_h264")]
    FfmpegH264,

    #[strum(to_string = "ffmpeg_vp8")]
    FfmpegVp8,

    #[strum(to_string = "ffmpeg_vp9")]
    FfmpegVp9,
}

#[derive(EnumIter, Display)]
pub enum AudioSetupOptions {
    #[strum(to_string = "Decoder")]
    Decoder,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(Display, EnumIter)]
pub enum AudioDecoder {
    #[strum(to_string = "opus")]
    Opus,

    #[strum(to_string = "aac")]
    Aac,
}
