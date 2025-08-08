use anyhow::Result;
use inquire::Select;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};

use crate::utils::{
    get_free_port,
    inputs::{
        input_name, AudioDecoder, AudioSetupOptions, InputHandler, VideoDecoder, VideoSetupOptions,
    },
    TransportProtocol,
};

#[derive(Debug, Display, EnumIter, Clone)]
pub enum RtpRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Set transport protocol")]
    SetTransportProtocol,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(Debug)]
pub struct RtpInput {
    name: String,
    port: u16,
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: Option<TransportProtocol>,
}

impl RtpInput {
    pub fn setup() -> Result<Self> {
        let mut rtp_input = Self {
            name: input_name(),

            // TODO: (@jbrs) Make it possible for user
            // to setup their own port
            port: get_free_port(),
            video: None,
            audio: None,
            transport_protocol: None,
        };

        let options = RtpRegisterOptions::iter().collect::<Vec<_>>();
        loop {
            let action = Select::new("What to do?", options.clone()).prompt()?;

            match action {
                RtpRegisterOptions::SetVideoStream => rtp_input.setup_video()?,
                RtpRegisterOptions::SetAudioStream => rtp_input.setup_audio()?,
                RtpRegisterOptions::SetTransportProtocol => rtp_input.setup_transport_protocol()?,
                RtpRegisterOptions::Done => {
                    if rtp_input.video.is_none() && rtp_input.audio.is_none() {
                        println!("At least one of \"video\" and \"audio\" has to be defined.");
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(rtp_input)
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

    fn setup_transport_protocol(&mut self) -> Result<()> {
        let options = TransportProtocol::iter().collect();

        let prot = Select::new("Select protocol:", options).prompt()?;
        self.transport_protocol = Some(prot);
        Ok(())
    }
}

impl InputHandler for RtpInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn serialize(&self) -> serde_json::Value {
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "transport_protocol": self.transport_protocol.as_ref().map(|t| t.to_string()),
            "video": self.video.as_ref().map(|v| v.serialize()),
            "audio": self.audio.as_ref().map(|a| a.serialize()),
        })
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

    pub fn serialize(&self) -> serde_json::Value {
        json!({
            "decoder": self.decoder.to_string(),
        })
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

    pub fn serialize(&self) -> serde_json::Value {
        json!({
            "decoder": self.decoder.to_string(),
        })
    }
}

impl Default for RtpInputAudioOptions {
    fn default() -> Self {
        Self {
            decoder: AudioDecoder::Opus,
        }
    }
}
