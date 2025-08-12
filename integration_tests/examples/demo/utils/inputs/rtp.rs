use anyhow::{anyhow, Result};
use inquire::Select;
use integration_tests::{
    ffmpeg::start_ffmpeg_send,
    gstreamer::{start_gst_send_tcp, start_gst_send_udp},
};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::utils::{
    get_free_port,
    inputs::{input_name, AudioDecoder, InputHandler, VideoDecoder},
    TransportProtocol, IP,
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
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: TransportProtocol,
}

impl RtpInput {
    pub fn setup() -> Result<Self> {
        let mut rtp_input = Self {
            name: input_name(),

            // TODO: (@jbrs) Make it possible for user
            // to set their own port
            port: get_free_port(),
            video: None,
            audio: None,
            transport_protocol: TransportProtocol::Udp,
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
                        error!("At least one of \"video\" and \"audio\" has to be defined.");
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
        self.transport_protocol = prot;
        Ok(())
    }

    fn gstreamer_transmit_tcp(&self) -> Result<()> {
        let video_port = self.video.as_ref().map(|_| self.port);
        let audio_port = self.audio.as_ref().map(|_| self.port);
        start_gst_send_tcp(
            IP,
            video_port,
            audio_port,
            integration_tests::examples::TestSample::ElephantsDreamH264Opus,
        )
    }

    fn gstreamer_transmit_udp(&self) -> Result<()> {
        let video_port = self.video.as_ref().map(|_| self.port);
        let audio_port = self.audio.as_ref().map(|_| self.port);
        start_gst_send_udp(
            IP,
            video_port,
            audio_port,
            integration_tests::examples::TestSample::ElephantsDreamH264Opus,
        )
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
            "transport_protocol": self.transport_protocol.to_string(),
            "video": self.video.as_ref().map(|v| v.serialize()),
            "audio": self.audio.as_ref().map(|a| a.serialize()),
        })
    }

    fn start_ffmpeg_transmitter(&self) -> Result<()> {
        if self.transport_protocol == TransportProtocol::TcpServer {
            return Err(anyhow!("FFmpeg cannot handle TCP connection."));
        }
        match (&self.video, &self.audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP."
                ));
            }
            (Some(_video), None) => start_ffmpeg_send(
                IP,
                Some(self.port),
                None,
                integration_tests::examples::TestSample::ElephantsDreamH264Opus,
            )?,
            (None, Some(_audio)) => start_ffmpeg_send(
                IP,
                None,
                Some(self.port),
                integration_tests::examples::TestSample::ElephantsDreamH264Opus,
            )?,
            (None, None) => return Err(anyhow!("No stream specified, ffmpeg not started!")),
        }
        Ok(())
    }

    fn start_gstreamer_transmitter(&self) -> Result<()> {
        match self.transport_protocol {
            TransportProtocol::Udp => self.gstreamer_transmit_udp(),
            TransportProtocol::TcpServer => self.gstreamer_transmit_tcp(),
        }
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
