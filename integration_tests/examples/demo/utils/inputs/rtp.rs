use anyhow::Result;
use inquire::Select;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};

use crate::utils::{
    get_free_port,
    inputs::{input_name, AudioDecoder, InputHandler, InputProtocol, VideoDecoder},
    TransportProtocol,
};

#[derive(Debug, Display, EnumIter, Clone)]
pub enum RtpRegisterOptions {
    #[strum(to_string = "Add video stream")]
    AddVideoStream,

    #[strum(to_string = "Add audio stream")]
    AddAudioStream,

    #[strum(to_string = "Remove video stream")]
    RemoveVideoStream,

    #[strum(to_string = "Remove audio stream")]
    RemoveAudioStream,

    #[strum(to_string = "Set transport protocol")]
    SetTransportProtocol,

    #[strum(to_string = "Done")]
    Done,
}

#[derive(Debug)]
pub struct RtpInput {
    name: String,
    port: u16,
    protocol: InputProtocol,
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: Option<TransportProtocol>,
}

impl RtpInput {
    pub fn setup() -> Result<Self> {
        let mut rtp_input = Self {
            name: input_name(),

            // TODO: (@jbrs) Make it possible for user
            // to set their own port
            port: get_free_port(),
            protocol: InputProtocol::Rtp,
            video: None,
            audio: None,
            transport_protocol: None,
        };

        let options = RtpRegisterOptions::iter().collect::<Vec<_>>();
        loop {
            let action = Select::new("What to do?", options.clone()).prompt()?;

            match action {
                RtpRegisterOptions::AddVideoStream => rtp_input.setup_video()?,
                RtpRegisterOptions::AddAudioStream => rtp_input.setup_audio()?,
                RtpRegisterOptions::RemoveVideoStream => {
                    rtp_input.video = None;
                    println!("Video stream removed!");
                }
                RtpRegisterOptions::RemoveAudioStream => {
                    rtp_input.audio = None;
                    println!("Audio stream removed!");
                }
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
        match self.video {
            Some(_) => println!("Video stream reset to default!"),
            None => println!("Video stream added!"),
        }
        self.video = Some(RtpInputVideoOptions::default());
        Ok(())
    }

    fn setup_audio(&mut self) -> Result<()> {
        match self.audio {
            Some(_) => println!("Audio stream reset to default!"),
            None => println!("Audio stream added!"),
        }
        self.audio = Some(RtpInputAudioOptions::default());
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

    fn protocol(&self) -> InputProtocol {
        self.protocol
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
