use anyhow::{Ok, Result};
use enum_iterator::{all, Sequence};
use inquire::{min_length, Select, Text};
use std::fmt::Display;
use std::process;
use tracing::error;

use crate::utils::inputs::{AudioDecoder, InputHandler, VideoDecoder};

#[derive(Sequence, Clone)]
enum RegisterOptions {
    SetVideoStream,
    SetAudioStream,
    Done,
}

impl Display for RegisterOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::SetVideoStream => "Set video stream",
            Self::SetAudioStream => "Set audio stream",
            Self::Done => "Done",
        };
        write!(f, "{msg}")
    }
}

pub struct RtpInput {
    pub name: String,
    pub video: Option<RtpInputVideoOptions>,
    pub audio: Option<RtpInputAudioOptions>,
}

impl RtpInput {
    pub fn setup() -> Result<Self> {
        let name_validator = min_length!(1, "Please enter a valid input name.");
        let name = Text::new("Enter input name: ")
            .with_validator(name_validator)
            .prompt()?;
        let mut rtp_input = Self {
            name,
            video: None,
            audio: None,
        };

        let options = all::<RegisterOptions>().collect::<Vec<_>>();
        loop {
            let action = Select::new("What to do?", options.clone()).prompt()?;

            match action {
                RegisterOptions::SetVideoStream => rtp_input.setup_video()?,
                RegisterOptions::SetAudioStream => rtp_input.setup_audio()?,
                RegisterOptions::Done => break,
            }
        }

        Ok(rtp_input)
    }
}

impl InputHandler for RtpInput {
    fn setup_video(&mut self) -> Result<()> {
        Ok(())
    }

    fn setup_audio(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct RtpInputVideoOptions {
    pub decoder: VideoDecoder,
}

impl Default for RtpInputVideoOptions {
    fn default() -> Self {
        Self {
            decoder: VideoDecoder::FfmpegH264,
        }
    }
}

pub struct RtpInputAudioOptions {
    pub decoder: AudioDecoder,
}

impl Default for RtpInputAudioOptions {
    fn default() -> Self {
        Self {
            decoder: AudioDecoder::Aac,
        }
    }
}
