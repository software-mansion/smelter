use std::process::Child;

use anyhow::{anyhow, Result};
use inquire::Select;
use integration_tests::{
    ffmpeg::start_ffmpeg_send,
    gstreamer::{start_gst_send_tcp, start_gst_send_udp},
};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{AudioDecoder, InputHandler, VideoDecoder},
    players::InputPlayerOptions,
    smelter_state::TransportProtocol,
    IP,
};

use crate::generators::get_free_port;

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
pub struct RtpInput {
    name: String,
    port: u16,
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: Option<TransportProtocol>,
    stream_handles: Vec<Child>,
}

impl RtpInput {
    fn gstreamer_transmit_tcp(&mut self) -> Result<()> {
        let video_port = self.video.as_ref().map(|_| self.port);
        let audio_port = self.audio.as_ref().map(|_| self.port);

        if video_port.is_some() && audio_port.is_some() {
            return Err(anyhow!(
                "Streaming both audio and video on the same port is possible only over UDP!"
            ));
        }
        self.stream_handles.push(start_gst_send_tcp(
            IP,
            video_port,
            audio_port,
            integration_tests::examples::TestSample::ElephantsDreamH264Opus,
        )?);
        Ok(())
    }

    fn gstreamer_transmit_udp(&mut self) -> Result<()> {
        let video_port = self.video.as_ref().map(|_| self.port);
        let audio_port = self.audio.as_ref().map(|_| self.port);
        self.stream_handles.push(start_gst_send_udp(
            IP,
            video_port,
            audio_port,
            integration_tests::examples::TestSample::ElephantsDreamH264Opus,
        )?);
        Ok(())
    }

    fn ffmpeg_transmit(&mut self) -> Result<()> {
        let (video_handle, audio_handle) = match (&self.video, &self.audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP."
                ))
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
        };

        if let Some(handle) = video_handle {
            self.stream_handles.push(handle);
        }
        if let Some(handle) = audio_handle {
            self.stream_handles.push(handle);
        }

        Ok(())
    }

    fn on_after_registration_udp(&mut self) -> Result<()> {
        let options = InputPlayerOptions::iter().collect::<Vec<_>>();

        loop {
            let player_choice = Select::new("Select player:", options.clone()).prompt()?;

            let player_result = match player_choice {
                InputPlayerOptions::StartFfmpegTransmitter => self.ffmpeg_transmit(),
                InputPlayerOptions::StartGstreamerTransmitter => self.gstreamer_transmit_udp(),
                InputPlayerOptions::Manual => Ok(()),
            };

            match player_result {
                Ok(_) => break,
                Err(e) => error!("{e}"),
            }
        }
        Ok(())
    }

    fn on_after_registration_tcp(&mut self) -> Result<()> {
        let options = vec![
            InputPlayerOptions::StartGstreamerTransmitter,
            InputPlayerOptions::Manual,
        ];

        loop {
            let player_choice = Select::new("Select player:", options.clone()).prompt()?;

            let player_result = match player_choice {
                InputPlayerOptions::StartGstreamerTransmitter => self.gstreamer_transmit_tcp(),
                InputPlayerOptions::Manual => break,
                _ => unreachable!(),
            };

            match player_result {
                Ok(_) => break,
                Err(e) => error!("{e}"),
            }
        }
        Ok(())
    }
}

impl InputHandler for RtpInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_after_registration(&mut self) -> Result<()> {
        match self.transport_protocol {
            Some(TransportProtocol::TcpServer) => self.on_after_registration_tcp(),
            Some(TransportProtocol::Udp) | None => self.on_after_registration_udp(),
        }
    }
}

impl Drop for RtpInput {
    fn drop(&mut self) {
        for stream_process in &mut self.stream_handles {
            match stream_process.kill() {
                Ok(_) => {}
                Err(e) => error!("{e}"),
            }
        }
    }
}

pub struct RtpInputBuilder {
    name: String,
    port: u16,
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: Option<TransportProtocol>,
}

impl RtpInputBuilder {
    pub fn new() -> Self {
        let port = get_free_port();
        let name = format!("input_rtp_udp_{}", port);
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
                    builder.with_video(RtpInputVideoOptions::default())
                }
                Some(RtpRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            let audio_selection =
                Select::new("Add audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(RtpRegisterOptions::AddAudioStream) => {
                    builder.with_audio(RtpInputAudioOptions::default())
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

    pub fn with_video(mut self, video: RtpInputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: RtpInputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_transport_protocol(mut self, transport_protocol: TransportProtocol) -> Self {
        match transport_protocol {
            TransportProtocol::Udp => {
                self.name = format!("input_rtp_udp_{}", self.port);
            }
            TransportProtocol::TcpServer => {
                self.name = format!("input_rtp_tcp_{}", self.port);
            }
        }
        self.transport_protocol = Some(transport_protocol);
        self
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

    pub fn build(self) -> (RtpInput, serde_json::Value) {
        let register_request = self.serialize();
        let rtp_input = RtpInput {
            name: self.name,
            port: self.port,
            video: self.video,
            audio: self.audio,
            transport_protocol: self.transport_protocol,
            stream_handles: vec![],
        };

        (rtp_input, register_request)
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
