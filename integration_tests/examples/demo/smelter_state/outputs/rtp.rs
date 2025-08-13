use std::process::Child;

use anyhow::{anyhow, Result};
use inquire::Select;
use integration_tests::{
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_receive_vp8, start_ffmpeg_receive_vp9},
    gstreamer::{
        start_gst_receive_tcp_h264, start_gst_receive_tcp_vp8, start_gst_receive_tcp_vp9,
        start_gst_receive_tcp_without_video, start_gst_receive_udp_h264, start_gst_receive_udp_vp8,
        start_gst_receive_udp_vp9, start_gst_receive_udp_without_video,
    },
};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::smelter_state::{
    get_free_port,
    outputs::{AudioEncoder, OutputHandler, VideoEncoder, VideoResolution},
    players::OutputPlayerOptions,
    TransportProtocol, IP,
};

#[derive(Debug, Display, EnumIter, Clone)]
pub enum RtpRegisterOptions {
    #[strum(to_string = "Add video stream")]
    AddVideoStream,

    #[strum(to_string = "Add audio stream")]
    AddAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug)]
pub struct RtpOutput {
    name: String,
    port: u16,
    video: Option<RtpOutputVideoOptions>,
    audio: Option<RtpOutputAudioOptions>,
    transport_protocol: Option<TransportProtocol>,
    stream_handles: Vec<Child>,
}

impl RtpOutput {
    fn start_gst_recv_tcp(&mut self) -> Result<()> {
        if self.video.is_none() && self.audio.is_none() {
            return Err(anyhow!("No stream specified, GStreamer not started!"));
        }
        match &self.video {
            Some(video) => {
                let audio = self.audio.is_some();
                match video.encoder {
                    VideoEncoder::FfmpegH264 => self
                        .stream_handles
                        .push(start_gst_receive_tcp_h264(IP, self.port, audio)?),
                    VideoEncoder::FfmpegVp8 => self
                        .stream_handles
                        .push(start_gst_receive_tcp_vp8(IP, self.port, audio)?),
                    VideoEncoder::FfmpegVp9 => self
                        .stream_handles
                        .push(start_gst_receive_tcp_vp9(IP, self.port, audio)?),
                }
            }
            None => self
                .stream_handles
                .push(start_gst_receive_tcp_without_video(IP, self.port, true)?),
        }
        Ok(())
    }

    fn start_gst_recv_udp(&mut self) -> Result<()> {
        if self.video.is_none() && self.audio.is_none() {
            return Err(anyhow!("No stream specified, GStreamer not started!"));
        }
        match &self.video {
            Some(video) => {
                if self.audio.is_some() {
                    return Err(anyhow!("Receiving both audio and video on the same port is possible only over TCP!"));
                }
                match video.encoder {
                    VideoEncoder::FfmpegH264 => self
                        .stream_handles
                        .push(start_gst_receive_udp_h264(self.port, false)?),
                    VideoEncoder::FfmpegVp8 => self
                        .stream_handles
                        .push(start_gst_receive_udp_vp8(self.port, false)?),
                    VideoEncoder::FfmpegVp9 => self
                        .stream_handles
                        .push(start_gst_receive_udp_vp9(self.port, false)?),
                }
            }
            None => self
                .stream_handles
                .push(start_gst_receive_udp_without_video(self.port, true)?),
        }
        Ok(())
    }

    fn start_ffmpeg_receiver(&mut self) -> Result<()> {
        if self.transport_protocol == Some(TransportProtocol::TcpServer) {
            return Err(anyhow!("FFmpeg cannot handle TCP connection."));
        }
        match (&self.video, &self.audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP."
                ))
            }
            (Some(video), None) => match video.encoder {
                VideoEncoder::FfmpegH264 => self
                    .stream_handles
                    .push(start_ffmpeg_receive_h264(Some(self.port), None)?),
                VideoEncoder::FfmpegVp8 => self
                    .stream_handles
                    .push(start_ffmpeg_receive_vp8(Some(self.port), None)?),
                VideoEncoder::FfmpegVp9 => self
                    .stream_handles
                    .push(start_ffmpeg_receive_vp9(Some(self.port), None)?),
            },
            (None, Some(_audio)) => self
                .stream_handles
                .push(start_ffmpeg_receive_h264(None, Some(self.port))?),
            (None, None) => return Err(anyhow!("No stream specified, ffmpeg not started!")),
        }
        Ok(())
    }
}

impl OutputHandler for RtpOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value {
        json!({
           "video": self.video.as_ref().map(|v| v.serialize_update(inputs)),
           "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }

    fn on_before_registration(&mut self) -> Result<()> {
        if self.transport_protocol == Some(TransportProtocol::Udp)
            || self.transport_protocol.is_none()
        {
            let options = OutputPlayerOptions::iter().collect::<Vec<_>>();

            loop {
                let player_choice = Select::new("Select player:", options.clone()).prompt()?;

                let player_result: Result<()> = match player_choice {
                    OutputPlayerOptions::StartFfmpegReceiver => self.start_ffmpeg_receiver(),
                    OutputPlayerOptions::StartGstreamerReceiver => self.start_gst_recv_udp(),
                    OutputPlayerOptions::Manual => Ok(()),
                };

                match player_result {
                    Ok(_) => break,
                    Err(e) => error!("{e}"),
                }
            }
        }
        Ok(())
    }

    fn on_after_registration(&mut self) -> Result<()> {
        if self.transport_protocol == Some(TransportProtocol::TcpServer) {
            let options = OutputPlayerOptions::iter()
                .filter(|o| *o != OutputPlayerOptions::StartFfmpegReceiver)
                .collect::<Vec<_>>();

            loop {
                let player_choice = Select::new("Select player:", options.clone()).prompt()?;

                let player_result: Result<()> = match player_choice {
                    OutputPlayerOptions::StartGstreamerReceiver => self.start_gst_recv_tcp(),
                    OutputPlayerOptions::Manual => Ok(()),
                    _ => unreachable!(),
                };

                match player_result {
                    Ok(_) => break,
                    Err(e) => error!("{e}"),
                }
            }
        }
        Ok(())
    }
}

impl Drop for RtpOutput {
    fn drop(&mut self) {
        for stream_process in &mut self.stream_handles {
            match stream_process.kill() {
                Ok(_) => {}
                Err(e) => error!("{e}"),
            }
        }
    }
}

pub struct RtpOutputBuilder {
    name: String,
    port: u16,
    video: Option<RtpOutputVideoOptions>,
    audio: Option<RtpOutputAudioOptions>,
    transport_protocol: Option<TransportProtocol>,
}

impl RtpOutputBuilder {
    pub fn new() -> Self {
        let port = get_free_port();
        let name = format!("output_rtp_udp_{port}");
        Self {
            name,
            port,
            video: None,
            audio: None,
            transport_protocol: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;
        let video_options = vec![RtpRegisterOptions::AddVideoStream, RtpRegisterOptions::Skip];
        let audio_options = vec![RtpRegisterOptions::AddAudioStream, RtpRegisterOptions::Skip];

        loop {
            let video_selection =
                Select::new("Add video stream?", video_options.clone()).prompt_skippable()?;

            builder = match video_selection {
                Some(RtpRegisterOptions::AddVideoStream) => {
                    builder.with_video(RtpOutputVideoOptions::default())
                }
                Some(RtpRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            let audio_selection =
                Select::new("Add audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(RtpRegisterOptions::AddAudioStream) => {
                    builder.with_audio(RtpOutputAudioOptions::default())
                }
                Some(RtpRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            if builder.video.is_none() && builder.audio.is_none() {
                error!("At least one video or one audio stream has to be specified!");
            } else {
                break;
            }
        }

        let transport_options = TransportProtocol::iter().collect();
        let transport_selection =
            Select::new("Select transport protocol?", transport_options).prompt_skippable()?;

        builder = match transport_selection {
            Some(prot) => builder.with_transport_protocol(prot),
            None => builder,
        };

        Ok(builder)
    }

    pub fn with_video(mut self, video: RtpOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: RtpOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_transport_protocol(mut self, transport_protocol: TransportProtocol) -> Self {
        match transport_protocol {
            TransportProtocol::Udp => {
                self.name = format!("output_rtp_udp_{}", self.port);
            }
            TransportProtocol::TcpServer => {
                self.name = format!("output_rtp_tcp_{}", self.port);
            }
        }
        self.transport_protocol = Some(transport_protocol);
        self
    }

    fn serialize(&self, inputs: &[&str]) -> serde_json::Value {
        let ip = match self.transport_protocol {
            Some(TransportProtocol::Udp) | None => Some(IP),
            Some(TransportProtocol::TcpServer) => None,
        };
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "ip": ip,
            "transport_protocol": self.transport_protocol.as_ref().map(|t| t.to_string()),
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    pub fn build(self, inputs: &[&str]) -> (RtpOutput, serde_json::Value) {
        let register_request = self.serialize(inputs);
        let rtp_output = RtpOutput {
            name: self.name,
            port: self.port,
            video: self.video,
            audio: self.audio,
            transport_protocol: self.transport_protocol,
            stream_handles: vec![],
        };
        (rtp_output, register_request)
    }
}

#[derive(Debug)]
pub struct RtpOutputVideoOptions {
    pub resolution: VideoResolution,
    pub encoder: VideoEncoder,
}

impl RtpOutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[&str]) -> serde_json::Value {
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
                    "transition": {
                        "duration_ms": 500,
                    },
                    "children": input_json,
                }
            }
        })
    }

    pub fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value {
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
            "root": {
                "type": "tiles",
                "id": "tiles",
                "transition": {
                    "duration_ms": 500,
                },
                "children": input_json,
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
    pub fn serialize_register(&self, inputs: &[&str]) -> serde_json::Value {
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

    pub fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value {
        let inputs_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "inputs": inputs_json,
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
