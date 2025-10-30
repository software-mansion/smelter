use std::process::Child;

use anyhow::{Result, anyhow};
use inquire::{Confirm, Select};
use integration_tests::{
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_receive_vp8, start_ffmpeg_receive_vp9},
    gstreamer::{
        start_gst_receive_tcp_h264, start_gst_receive_tcp_vp8, start_gst_receive_tcp_vp9,
        start_gst_receive_tcp_without_video, start_gst_receive_udp_h264, start_gst_receive_udp_vp8,
        start_gst_receive_udp_vp9, start_gst_receive_udp_without_video,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    IP,
    inputs::{InputHandle, filter_video_inputs},
    outputs::{AudioEncoder, OutputHandle, VideoEncoder, VideoResolution, scene::Scene},
    players::OutputPlayer,
};

use crate::smelter_state::TransportProtocol;
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(from = "RtpOutputDeserialize")]
pub struct RtpOutput {
    #[serde(skip_serializing)]
    name: String,

    #[serde(skip_serializing)]
    port: u16,
    video: Option<RtpOutputVideoOptions>,
    audio: Option<RtpOutputAudioOptions>,
    transport_protocol: TransportProtocol,

    #[serde(skip)]
    stream_handles: Vec<Child>,
    player: OutputPlayer,
}

#[derive(Debug, Deserialize)]
pub struct RtpOutputDeserialize {
    video: Option<RtpOutputVideoOptions>,
    audio: Option<RtpOutputAudioOptions>,
    transport_protocol: TransportProtocol,
    player: OutputPlayer,
}

impl From<RtpOutputDeserialize> for RtpOutput {
    fn from(value: RtpOutputDeserialize) -> Self {
        let port = get_free_port();
        let name = format!("output_rtp_{}_{port}", value.transport_protocol);
        Self {
            name,
            port,
            video: value.video,
            audio: value.audio,
            transport_protocol: value.transport_protocol,
            stream_handles: vec![],
            player: value.player,
        }
    }
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
                    _ => return Err(anyhow!("Invalid encoder for RTP output.")),
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
                    return Err(anyhow!(
                        "Receiving both audio and video on the same port is possible only over TCP!"
                    ));
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
                    _ => return Err(anyhow!("Invalid encoder for RTP output.")),
                }
            }
            None => self
                .stream_handles
                .push(start_gst_receive_udp_without_video(self.port, true)?),
        }
        Ok(())
    }

    fn start_ffmpeg_receiver(&mut self) -> Result<()> {
        if self.transport_protocol == TransportProtocol::TcpServer {
            return Err(anyhow!("FFmpeg cannot handle TCP connection."));
        }
        match (&self.video, &self.audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP."
                ));
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
                _ => return Err(anyhow!("Invalid encoder for RTP output.")),
            },
            (None, Some(_audio)) => self
                .stream_handles
                .push(start_ffmpeg_receive_h264(None, Some(self.port))?),
            (None, None) => return Err(anyhow!("No stream specified, ffmpeg not started!")),
        }
        Ok(())
    }
}

#[typetag::serde]
impl OutputHandle for RtpOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let ip = match self.transport_protocol {
            TransportProtocol::Udp => Some(IP),
            TransportProtocol::TcpServer => None,
        };
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "ip": ip,
            "transport_protocol": self.transport_protocol.to_string(),
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    fn serialize_update(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        json!({
           "video": self.video.as_ref().map(|v| v.serialize_update(inputs)),
           "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }

    fn on_before_registration(&mut self) -> Result<()> {
        match self.transport_protocol {
            TransportProtocol::Udp => match self.player {
                OutputPlayer::Ffmpeg => self.start_ffmpeg_receiver(),
                OutputPlayer::Gstreamer => self.start_gst_recv_udp(),
                OutputPlayer::Manual => {
                    match (&self.video, &self.audio) {
                        (Some(video), Some(_)) => {
                            let cmd = build_gst_recv_udp_cmd(Some(video.encoder), true, self.port);
                            println!(
                                "Start stream receiver for H264 encoded video and OPUS encoded audio:"
                            );
                            println!("{cmd}");
                            println!();
                        }
                        (Some(video), None) => {
                            let cmd = build_gst_recv_udp_cmd(Some(video.encoder), false, self.port);
                            println!("Start stream receiver for H264 encoded video:");
                            println!("{cmd}");
                            println!();
                        }
                        (None, Some(_)) => {
                            let cmd = build_gst_recv_udp_cmd(None, true, self.port);
                            println!("Start stream receiver for OPUS encoded audio");
                            println!("{cmd}");
                            println!();
                        }
                        _ => unreachable!(),
                    }

                    loop {
                        let confirmation = Confirm::new("Is player running? [Y/n]")
                            .with_default(true)
                            .prompt()?;
                        if confirmation {
                            return Ok(());
                        }
                    }
                }
            },
            TransportProtocol::TcpServer => Ok(()),
        }
    }

    fn on_after_registration(&mut self) -> Result<()> {
        match self.transport_protocol {
            TransportProtocol::TcpServer => match self.player {
                OutputPlayer::Gstreamer => self.start_gst_recv_tcp(),
                OutputPlayer::Manual => {
                    match (&self.video, &self.audio) {
                        (Some(video), Some(_)) => {
                            let cmd = build_gst_recv_tcp_cmd(Some(video.encoder), true, self.port);
                            println!(
                                "Start stream receiver for H264 encoded video and OPUS encoded audio:"
                            );
                            println!("{cmd}");
                            println!();
                        }
                        (Some(video), None) => {
                            let cmd = build_gst_recv_tcp_cmd(Some(video.encoder), false, self.port);
                            println!("Start stream receiver for H264 encoded video:");
                            println!("{cmd}");
                            println!();
                        }
                        (None, Some(_)) => {
                            let cmd = build_gst_recv_tcp_cmd(None, true, self.port);
                            println!("Start stream receiver for OPUS encoded audio:");
                            println!("{cmd}");
                            println!();
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                }
                _ => Err(anyhow!("Invalid player for RTP in TCP server mode.")),
            },
            TransportProtocol::Udp => Ok(()),
        }
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
    transport_protocol: TransportProtocol,
    player: OutputPlayer,
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
            transport_protocol: TransportProtocol::Udp,
            player: OutputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        loop {
            builder = builder.prompt_video()?.prompt_audio()?;

            if builder.video.is_none() && builder.audio.is_none() {
                error!("At least one video or one audio stream has to be specified!");
            } else {
                break;
            }
        }

        builder.prompt_transport_protocol()?.prompt_player()
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![RtpRegisterOptions::SetVideoStream, RtpRegisterOptions::Skip];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(RtpRegisterOptions::SetVideoStream) => {
                let mut video = RtpOutputVideoOptions::default();

                let scene_options = Scene::iter().collect();
                let scene_choice = Select::new("Select scene (ESC for Tiles):", scene_options)
                    .prompt_skippable()?;
                if let Some(scene) = scene_choice {
                    video.scene = scene;
                }

                let encoder_options = VideoEncoder::iter()
                    .filter(|enc| *enc != VideoEncoder::Any)
                    .collect();
                let encoder_choice =
                    Select::new("Select encoder (ESC for ffmpeg_h264):", encoder_options)
                        .prompt_skippable()?;
                if let Some(enc) = encoder_choice {
                    video.encoder = enc;
                }

                Ok(self.with_video(video))
            }
            Some(RtpRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![RtpRegisterOptions::SetAudioStream, RtpRegisterOptions::Skip];
        let audio_selection = Select::new("Set audio stream?", audio_options).prompt_skippable()?;

        match audio_selection {
            Some(RtpRegisterOptions::SetAudioStream) => {
                Ok(self.with_audio(RtpOutputAudioOptions::default()))
            }
            Some(RtpRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_transport_protocol(self) -> Result<Self> {
        let transport_options = TransportProtocol::iter().collect();
        let transport_selection =
            Select::new("Select transport protocol (ESC for UDP)", transport_options)
                .prompt_skippable()?;

        match transport_selection {
            Some(prot) => Ok(self.with_transport_protocol(prot)),
            None => Ok(self),
        }
    }

    fn prompt_player(self) -> Result<Self> {
        match self.transport_protocol {
            TransportProtocol::Udp => {
                let (player_options, default_player) = match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => (vec![OutputPlayer::Manual], OutputPlayer::Manual),
                    _ => (OutputPlayer::iter().collect(), OutputPlayer::Gstreamer),
                };
                let player_selection = Select::new(
                    &format!("Select player (ESC for {default_player}):"),
                    player_options,
                )
                .prompt_skippable()?;
                match player_selection {
                    Some(player) => Ok(self.with_player(player)),
                    None => Ok(self.with_player(default_player)),
                }
            }
            TransportProtocol::TcpServer => {
                let player_options = vec![OutputPlayer::Gstreamer, OutputPlayer::Manual];
                let player_selection =
                    Select::new("Select player (ESC for GStreamer):", player_options)
                        .prompt_skippable()?;
                match player_selection {
                    Some(player) => Ok(self.with_player(player)),
                    None => Ok(self.with_player(OutputPlayer::Gstreamer)),
                }
            }
        }
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
        self.transport_protocol = transport_protocol;
        self
    }

    pub fn with_player(mut self, player: OutputPlayer) -> Self {
        self.player = player;
        self
    }

    pub fn build(self) -> RtpOutput {
        RtpOutput {
            name: self.name,
            port: self.port,
            video: self.video,
            audio: self.audio,
            transport_protocol: self.transport_protocol,
            stream_handles: vec![],
            player: self.player,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RtpOutputVideoOptions {
    root_id: String,
    resolution: VideoResolution,
    encoder: VideoEncoder,
    scene: Scene,
}

impl RtpOutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);

        json!({
            "resolution": self.resolution.serialize(),
            "encoder": {
                "type": self.encoder.to_string(),
            },
            "initial": {
                "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
            }
        })
    }

    pub fn serialize_update(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);

        json!({
            "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
        })
    }
}

impl Default for RtpOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        let root_id = "root".to_string();
        Self {
            root_id,
            resolution,
            encoder: VideoEncoder::FfmpegH264,
            scene: Scene::Tiles,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RtpOutputAudioOptions {
    pub encoder: AudioEncoder,
}

impl RtpOutputAudioOptions {
    pub fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs_json = inputs
            .iter()
            .filter_map(|input| {
                if input.has_audio() {
                    Some(json!({
                        "input_id": input.name(),
                    }))
                } else {
                    None
                }
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

    pub fn serialize_update(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs_json = inputs
            .iter()
            .filter_map(|input| {
                if input.has_audio() {
                    Some(json!({
                        "input_id": input.name(),
                    }))
                } else {
                    None
                }
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

fn build_gst_recv_tcp_cmd(video_codec: Option<VideoEncoder>, has_audio: bool, port: u16) -> String {
    if !has_audio && video_codec.is_none() {
        return String::new();
    }

    let base_cmd = format!(
        "gst-launch-1.0 -v rtpptdemux name=demux tcpclientsrc host='127.0.0.1' port={port} ! \"application/x-rtp-stream\" ! rtpstreamdepay ! queue ! demux. ",
    );

    let video_cmd = match video_codec {
        Some(VideoEncoder::FfmpegH264) => {
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=H264\" ! queue ! rtph264depay ! decodebin ! videoconvert ! autovideosink "
        }
        Some(VideoEncoder::FfmpegVp8) => {
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=VP8\" ! queue ! rtpvp8depay ! decodebin ! videoconvert ! autovideosink "
        }
        Some(VideoEncoder::FfmpegVp9) => {
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=VP9\" ! queue ! rtpvp9depay ! decodebin ! videoconvert ! autovideosink "
        }
        None => "",
        _ => unreachable!(),
    };

    let audio_cmd = match has_audio {
        true => {
            "demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink "
        }
        false => "",
    };

    base_cmd + video_cmd + audio_cmd
}

fn build_gst_recv_udp_cmd(video_codec: Option<VideoEncoder>, has_audio: bool, port: u16) -> String {
    if !has_audio && video_codec.is_none() {
        return String::new();
    }

    let base_cmd = format!(
        "gst-launch-1.0 -v rtpptdemux name=demux udpsrc port={port} ! \"application/x-rtp\" ! queue ! demux. ",
    );

    let video_cmd = match video_codec {
        Some(VideoEncoder::FfmpegH264) => {
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=H264\" ! queue ! rtph264depay ! decodebin ! videoconvert ! autovideosink "
        }
        Some(VideoEncoder::FfmpegVp8) => {
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=VP8\" ! queue ! rtpvp8depay ! decodebin ! videoconvert ! autovideosink "
        }
        Some(VideoEncoder::FfmpegVp9) => {
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=VP9\" ! queue ! rtpvp9depay ! decodebin ! videoconvert ! autovideosink "
        }
        None => "",
        _ => unreachable!(),
    };

    let audio_cmd = match has_audio {
        true => {
            "demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink sync=false"
        }
        false => "",
    };

    base_cmd + video_cmd + audio_cmd
}
