use anyhow::Result;
use inquire::{min_length, Select, Text};
use strum::IntoEnumIterator;

use crate::utils::{
    inputs::{AudioDecoder, InputHandler, VideoDecoder, VideoSetupOptions},
    RegisterOptions,
};

pub struct RtpInput {
    name: String,
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
}

impl RtpInput {
    pub fn setup() -> Result<Self> {
        let name_validator = min_length!(1, "Please enter a valid input name.");
        let name = Text::new("Enter input name:")
            .with_validator(name_validator)
            .prompt()?;
        let mut rtp_input = Self {
            name,
            video: None,
            audio: None,
        };

        let options = RegisterOptions::iter().collect::<Vec<_>>();
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
    fn name(&self) -> &str {
        &self.name
    }

    fn setup_video(&mut self) -> Result<()> {
        // let setup_options = all::<VideoSetupOptions>().collect();

        // let setup_choice = Select::new("Setup: ", setup_options).prompt();
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
