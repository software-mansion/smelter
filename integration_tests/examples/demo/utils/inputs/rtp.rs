use anyhow::Result;
use inquire::{min_length, Select, Text};
use strum::IntoEnumIterator;

use crate::utils::{
    inputs::{
        input_name, AudioDecoder, AudioSetupOptions, InputHandler, VideoDecoder, VideoSetupOptions,
    },
    RegisterOptions,
};

#[derive(Debug)]
pub struct RtpInput {
    name: String,
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
}

impl RtpInput {
    pub fn setup() -> Result<Self> {
        let mut rtp_input = Self {
            name: input_name(),
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
        self.video = Some(RtpInputVideoOptions::default());
        let setup_options = VideoSetupOptions::iter().collect::<Vec<_>>();

        loop {
            let setup_choice = Select::new("Setup:", setup_options.clone()).prompt()?;

            match setup_choice {
                VideoSetupOptions::Decoder => self.video.as_mut().unwrap().set_decoder_prompt()?,
                VideoSetupOptions::Done => break,
            }
        }
        Ok(())
    }

    fn setup_audio(&mut self) -> Result<()> {
        self.audio = Some(RtpInputAudioOptions::default());
        let setup_options = AudioSetupOptions::iter().collect::<Vec<_>>();

        loop {
            let setup_choice = Select::new("Setup:", setup_options.clone()).prompt()?;

            match setup_choice {
                AudioSetupOptions::Decoder => self.audio.as_mut().unwrap().set_decoder_prompt()?,
                AudioSetupOptions::Done => break,
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct RtpInputVideoOptions {
    pub decoder: VideoDecoder,
}

impl RtpInputVideoOptions {
    pub fn set_decoder_prompt(&mut self) -> Result<()> {
        let options = VideoDecoder::iter().collect();

        let decoder = Select::new("Select decoder:", options).prompt()?;
        self.decoder = decoder;
        Ok(())
    }
}

impl Default for RtpInputVideoOptions {
    fn default() -> Self {
        Self {
            decoder: VideoDecoder::FfmpegH264,
        }
    }
}

#[derive(Debug)]
pub struct RtpInputAudioOptions {
    pub decoder: AudioDecoder,
}

impl RtpInputAudioOptions {
    pub fn set_decoder_prompt(&mut self) -> Result<()> {
        let options = AudioDecoder::iter().collect();

        let decoder = Select::new("Select decoder:", options).prompt()?;
        self.decoder = decoder;
        Ok(())
    }
}

impl Default for RtpInputAudioOptions {
    fn default() -> Self {
        Self {
            decoder: AudioDecoder::Aac,
        }
    }
}
