use std::process::Child;

use anyhow::{anyhow, Result};
use inquire::{Confirm, Select};
use integration_tests::{
    ffmpeg::start_ffmpeg_send,
    gstreamer::{start_gst_send_tcp, start_gst_send_udp},
};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{AudioDecoder, InputHandler, VideoDecoder},
    players::InputPlayer,
    smelter_state::TransportProtocol,
    IP,
};

use crate::utils::get_free_port;

#[derive(Debug, Display, EnumIter, Clone)]
pub enum RtpRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

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
            integration_tests::examples::TestSample::BigBuckBunnyH264Opus,
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
            integration_tests::examples::TestSample::BigBuckBunnyH264Opus,
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
                integration_tests::examples::TestSample::BigBuckBunnyH264Opus,
            )?,
            (None, Some(_audio)) => start_ffmpeg_send(
                IP,
                None,
                Some(self.port),
                integration_tests::examples::TestSample::BigBuckBunnyH264Opus,
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

    fn on_after_registration_udp(&mut self, player: InputPlayer) -> Result<()> {
        match player {
            InputPlayer::FfmpegTransmitter => self.ffmpeg_transmit(),
            InputPlayer::GstreamerTransmitter => self.gstreamer_transmit_udp(),
            InputPlayer::Manual => {
                let cmd_base = [
                    "gst-launch-1.0 -v ",
                    "filesrc location=<PATH_TO_FILE> ! qtdemux name=demux ",
                ]
                .concat();

                let video_cmd = format!(" demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96  ! udpsink host='127.0.0.1' port={} ", self.port);
                let audio_cmd = format!("demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! udpsink host='127.0.0.1' port={} ", self.port);

                match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => {
                        let cmd = cmd_base + &video_cmd + &audio_cmd;
                        println!("Start streaming H264 encoded video and OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    (Some(_), None) => {
                        let cmd = cmd_base + &video_cmd;
                        println!("Start streaming H264 encoded video:");
                        println!("{cmd}");
                        println!();
                    }
                    (None, Some(_)) => {
                        let cmd = cmd_base + &audio_cmd;
                        println!("Start streaming OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    _ => unreachable!(),
                }

                loop {
                    let confirmation = Confirm::new("Is player running? [y/n]").prompt()?;
                    if confirmation {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn on_after_registration_tcp(&mut self, player: InputPlayer) -> Result<()> {
        match player {
            InputPlayer::GstreamerTransmitter => self.gstreamer_transmit_tcp(),
            InputPlayer::Manual => {
                let cmd_base = [
                    "gst-launch-1.0 -v ",
                    "filesrc location=<FILE_PATH> ! qtdemux name=demux ",
                ]
                .concat();
                let video_cmd = format!("demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96  ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={} ", self.port);
                let audio_cmd = format!("demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={}", self.port);

                match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => {
                        let cmd = cmd_base + &video_cmd + &audio_cmd;
                        println!("Start streaming H264 encoded video and OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    (Some(_), None) => {
                        let cmd = cmd_base + &video_cmd;
                        println!("Start streaming H264 encoded video:");
                        println!("{cmd}");
                        println!();
                    }
                    (None, Some(_)) => {
                        let cmd = cmd_base + &audio_cmd;
                        println!("Start streaming OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    _ => unreachable!(),
                }

                loop {
                    let confirmation = Confirm::new("Is player running? [y/n]").prompt()?;
                    if confirmation {
                        return Ok(());
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl InputHandler for RtpInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_after_registration(&mut self, player: InputPlayer) -> Result<()> {
        match self.transport_protocol {
            Some(TransportProtocol::TcpServer) => self.on_after_registration_tcp(player),
            Some(TransportProtocol::Udp) | None => self.on_after_registration_udp(player),
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
    player: InputPlayer,
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
            player: InputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;
        let video_options = vec![RtpRegisterOptions::SetVideoStream, RtpRegisterOptions::Skip];
        let audio_options = vec![RtpRegisterOptions::SetAudioStream, RtpRegisterOptions::Skip];

        loop {
            let video_selection =
                Select::new("Set video stream?", video_options.clone()).prompt_skippable()?;

            builder = match video_selection {
                Some(RtpRegisterOptions::SetVideoStream) => {
                    builder.with_video(RtpInputVideoOptions::default())
                }
                Some(RtpRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            let audio_selection =
                Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(RtpRegisterOptions::SetAudioStream) => {
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

        let player_selection = builder.prompt_player()?;
        builder = match player_selection {
            Some(player) => builder.with_player(player),
            None => builder,
        };
        Ok(builder)
    }

    fn prompt_player(&self) -> Result<Option<InputPlayer>> {
        match self.transport_protocol {
            Some(TransportProtocol::Udp) | None => {
                let player_options = match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => {
                        vec![InputPlayer::GstreamerTransmitter, InputPlayer::Manual]
                    }
                    _ => InputPlayer::iter().collect(),
                };

                let player_selection =
                    Select::new("Select player:", player_options).prompt_skippable()?;
                Ok(player_selection)
            }
            Some(TransportProtocol::TcpServer) => {
                let player_options = vec![InputPlayer::GstreamerTransmitter, InputPlayer::Manual];
                let player_selection =
                    Select::new("Select player:", player_options).prompt_skippable()?;
                Ok(player_selection)
            }
        }
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

    pub fn with_player(mut self, player: InputPlayer) -> Self {
        self.player = player;
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

    pub fn build(self) -> (RtpInput, serde_json::Value, InputPlayer) {
        let register_request = self.serialize();
        let rtp_input = RtpInput {
            name: self.name,
            port: self.port,
            video: self.video,
            audio: self.audio,
            transport_protocol: self.transport_protocol,
            stream_handles: vec![],
        };

        (rtp_input, register_request, self.player)
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
