use anyhow::{anyhow, Result};
use inquire::Select;
use integration_tests::ffmpeg::{
    start_ffmpeg_receive_h264, start_ffmpeg_receive_vp8, start_ffmpeg_receive_vp9,
};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{error, info};

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
    transport_protocol: TransportProtocol,
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
            transport_protocol: TransportProtocol::Udp,
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
                    info!("Video stream removed!");
                }
                RtpRegisterOptions::RemoveAudioStream => {
                    rtp_output.audio = None;
                    info!("Audio stream removed!");
                }
                RtpRegisterOptions::SetTransportProtocol => {
                    rtp_output.setup_transport_protocol()?
                }
                RtpRegisterOptions::Done => {
                    if rtp_output.video.is_none() && rtp_output.audio.is_none() {
                        error!("At least one of \"video\" and \"audio\" has to be defined.");
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
            Some(_) => info!("Video stream reset to default!"),
            None => info!("Video stream added!"),
        }
        self.video = Some(RtpOutputVideoOptions::default());
        Ok(())
    }

    fn setup_audio(&mut self) -> Result<()> {
        match self.audio {
            Some(_) => info!("Audio stream reset to default!"),
            None => info!("Audio stream added!"),
        }
        self.audio = Some(RtpOutputAudioOptions::default());
        Ok(())
    }

    fn setup_transport_protocol(&mut self) -> Result<()> {
        let options = TransportProtocol::iter().collect();

        let prot = Select::new("Select protocol:", options).prompt()?;
        self.transport_protocol = prot;
        Ok(())
    }

    pub fn start_ffmpeg_receiver(&self) -> Result<()> {
        if self.transport_protocol == TransportProtocol::TcpServer {
            return Err(anyhow!("FFmpeg cannot handle TCP connection."));
        }
        match (&self.video, &self.audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP"
                ));
            }
            (Some(video), None) => match video.encoder {
                VideoEncoder::FfmpegH264 => start_ffmpeg_receive_h264(Some(self.port), None)?,
                VideoEncoder::FfmpegVp8 => start_ffmpeg_receive_vp8(Some(self.port), None)?,
                VideoEncoder::FfmpegVp9 => start_ffmpeg_receive_vp9(Some(self.port), None)?,
            },
            (None, Some(_audio)) => start_ffmpeg_receive_h264(None, Some(self.port))?,
            (None, None) => return Err(anyhow!("No stream specified, ffmpeg not started!")),
        }
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

    fn transport_protocol(&self) -> TransportProtocol {
        self.transport_protocol
    }

    fn inputs(&mut self) -> &mut Vec<String> {
        &mut self.inputs
    }

    fn serialize(&self) -> serde_json::Value {
        let ip = match self.transport_protocol {
            TransportProtocol::Udp => Some("127.0.0.1"),
            TransportProtocol::TcpServer => None,
        };
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "ip": ip,
            "transport_protocol": self.transport_protocol.to_string(),
            "video": self.video.as_ref().map(|v| v.serialize(&self.inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize(&self.inputs)),
        })
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
