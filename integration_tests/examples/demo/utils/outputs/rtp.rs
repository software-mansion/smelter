use anyhow::Result;
use inquire::Select;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};

use crate::utils::{
    get_free_port,
    inputs::InputHandler,
    outputs::{
        output_name, AudioEncoder, OutputHandler, OutputProtocol, VideoEncoder, VideoResolution,
    },
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
pub struct RtpOutput {
    name: String,
    port: u16,
    protocol: OutputProtocol,
    video: Option<RtpOutputVideoOptions>,
    audio: Option<RtpOutputAudioOptions>,
    transport_protocol: Option<TransportProtocol>,
    inputs: Vec<String>,
}

impl RtpOutput {
    pub fn setup() -> Result<Self> {
        let mut rtp_output = Self {
            name: output_name(),

            // TODO: (@jbrs) Make it possible for user
            // to set their own port
            port: get_free_port(),
            protocol: OutputProtocol::Rtp,
            video: None,
            audio: None,
            transport_protocol: None,
            inputs: vec![],
        };

        let options = RtpRegisterOptions::iter().collect::<Vec<_>>();
        loop {
            let action = Select::new("What to do?", options.clone()).prompt()?;

            match action {
                RtpRegisterOptions::AddVideoStream => rtp_output.setup_video()?,
                RtpRegisterOptions::AddAudioStream => rtp_output.setup_audio()?,
                RtpRegisterOptions::RemoveVideoStream => {
                    rtp_output.video = None;
                    println!("Video stream removed!");
                }
                RtpRegisterOptions::RemoveAudioStream => {
                    rtp_output.audio = None;
                    println!("Audio stream removed!");
                }
                RtpRegisterOptions::SetTransportProtocol => {
                    rtp_output.setup_transport_protocol()?
                }
                RtpRegisterOptions::Done => {
                    if rtp_output.video.is_none() && rtp_output.audio.is_none() {
                        println!("At least one of \"video\" and \"audio\" has to be defined.");
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(rtp_output)
    }

    fn setup_video(&mut self) -> Result<()> {
        match self.video {
            Some(_) => println!("Video stream reset to default!"),
            None => println!("Video stream added!"),
        }
        self.video = Some(RtpOutputVideoOptions::default());
        Ok(())
    }

    fn setup_audio(&mut self) -> Result<()> {
        match self.audio {
            Some(_) => println!("Audio stream reset to default!"),
            None => println!("Audio stream added!"),
        }
        self.audio = Some(RtpOutputAudioOptions::default());
        Ok(())
    }

    fn setup_transport_protocol(&mut self) -> Result<()> {
        let options = TransportProtocol::iter().collect();

        let prot = Select::new("Select protocol:", options).prompt()?;
        self.transport_protocol = Some(prot);
        Ok(())
    }
}

impl OutputHandler for RtpOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn protocol(&self) -> OutputProtocol {
        self.protocol
    }

    fn serialize(&self) -> serde_json::Value {
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "ip": "127.0.0.1",
            "transport_protocol": self.transport_protocol.as_ref().map(|t| t.to_string()),
            "video": self.video.as_ref().map(|v| v.serialize(&self.inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize(&self.inputs)),
        })
    }

    fn set_initial_scene(&mut self, inputs: &[Box<dyn InputHandler>]) {
        for input in inputs {
            self.inputs.push(input.name().to_string());
        }
    }

    fn add_input(&mut self, input: &dyn InputHandler) {
        self.inputs.push(input.name().to_string());
    }

    fn remove_input(&mut self, input: &dyn InputHandler) {
        let index = self.inputs.iter().position(|name| name == input.name());
        if let Some(i) = index {
            self.inputs.remove(i);
        }
    }
}

#[derive(Debug)]
pub struct RtpOutputVideoOptions {
    pub resolution: VideoResolution,
    pub encoder: VideoEncoder,
}

impl RtpOutputVideoOptions {
    pub fn serialize(&self, inputs: &[String]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "type": "input_stream",
                    "id": input_id,
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "resolution": self.resolution.serialize(),
            "encoder": {
                "type": self.encoder.to_string(),
            },
            "initial": {
                "root": {
                    "type": "tiles",
                    "id": "tiles",
                    "children": input_json,
                }
            }
        })
    }
}

impl Default for RtpOutputVideoOptions {
    fn default() -> Self {
        Self {
            resolution: VideoResolution {
                width: 1920,
                height: 1080,
            },
            encoder: VideoEncoder::FfmpegH264,
        }
    }
}

#[derive(Debug)]
pub struct RtpOutputAudioOptions {
    pub encoder: AudioEncoder,
}

impl RtpOutputAudioOptions {
    pub fn serialize(&self, inputs: &[String]) -> serde_json::Value {
        let inputs_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "encoder": {
                "type": self.encoder.to_string(),
            },
            "initial": {
                "inputs": inputs_json,
        }
        })
    }
}

impl Default for RtpOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Opus,
        }
    }
}
