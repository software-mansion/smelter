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
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    IP,
    inputs::{InputHandle, filter_video_inputs},
    outputs::{AudioEncoder, VideoEncoder, VideoResolution, scene::Scene},
    players::OutputPlayer,
};

use crate::utils::get_free_port;

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportProtocol {
    #[strum(to_string = "udp")]
    Udp,

    #[strum(to_string = "tcp_server")]
    TcpServer,
}

#[derive(Debug, Display, EnumIter, Clone)]
pub enum RtpRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Deserialize)]
#[serde(from = "RtpOutputOptions")]
pub struct RtpOutput {
    pub name: String,
    port: u16,
    options: RtpOutputOptions,
    stream_handles: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtpOutputOptions {
    video: Option<RtpOutputVideoOptions>,
    audio: Option<RtpOutputAudioOptions>,
    transport_protocol: TransportProtocol,
    player: OutputPlayer,
}

impl Serialize for RtpOutput {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("RtpOutput", 4)?;
        state.serialize_field("video", &self.options.video)?;
        state.serialize_field("audio", &self.options.audio)?;
        state.serialize_field("transport_protocol", &self.options.transport_protocol)?;
        state.serialize_field("player", &self.options.player)?;
        state.end()
    }
}

impl From<RtpOutputOptions> for RtpOutput {
    fn from(value: RtpOutputOptions) -> Self {
        let port = get_free_port();
        let name = format!("output_rtp_{}_{port}", value.transport_protocol);
        Self {
            name,
            port,
            options: value,
            stream_handles: vec![],
        }
    }
}

impl RtpOutput {
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let RtpOutputOptions {
            ref video,
            ref audio,
            transport_protocol,
            ..
        } = self.options;
        let ip = match transport_protocol {
            TransportProtocol::Udp => Some(IP),
            TransportProtocol::TcpServer => None,
        };
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "ip": ip,
            "transport_protocol": transport_protocol.to_string(),
            "video": video.as_ref().map(|v| v.serialize_register(inputs)),
            "audio": audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
        json!({
           "video": self.options.video.as_ref().map(|v| v.serialize_update(inputs)),
           "audio": self.options.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }

    pub fn on_before_registration(&mut self) -> Result<()> {
        let RtpOutputOptions {
            ref video,
            ref audio,
            transport_protocol,
            player,
        } = self.options;
        match transport_protocol {
            TransportProtocol::Udp => match player {
                OutputPlayer::Ffmpeg => self.start_ffmpeg_receiver(),
                OutputPlayer::Gstreamer => self.start_gst_recv_udp(),
                OutputPlayer::Manual => {
                    match (video, audio) {
                        (Some(v), Some(_)) => {
                            let cmd = build_gst_recv_udp_cmd(Some(v.encoder), true, self.port);
                            println!(
                                "Start stream receiver for H264 encoded video and OPUS encoded audio:"
                            );
                            println!("{cmd}");
                            println!();
                        }
                        (Some(v), None) => {
                            let cmd = build_gst_recv_udp_cmd(Some(v.encoder), false, self.port);
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

    pub fn on_after_registration(&mut self) -> Result<()> {
        let RtpOutputOptions {
            ref video,
            ref audio,
            transport_protocol,
            player,
        } = self.options;
        match transport_protocol {
            TransportProtocol::TcpServer => match player {
                OutputPlayer::Gstreamer => self.start_gst_recv_tcp(),
                OutputPlayer::Manual => {
                    match (video, audio) {
                        (Some(v), Some(_)) => {
                            let cmd = build_gst_recv_tcp_cmd(Some(v.encoder), true, self.port);
                            println!(
                                "Start stream receiver for H264 encoded video and OPUS encoded audio:"
                            );
                            println!("{cmd}");
                            println!();
                        }
                        (Some(v), None) => {
                            let cmd = build_gst_recv_tcp_cmd(Some(v.encoder), false, self.port);
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

    fn start_gst_recv_tcp(&mut self) -> Result<()> {
        let RtpOutputOptions { video, audio, .. } = &self.options;
        if video.is_none() && audio.is_none() {
            return Err(anyhow!("No stream specified, GStreamer not started!"));
        }
        match video {
            Some(v) => {
                let a = audio.is_some();
                match v.encoder {
                    VideoEncoder::FfmpegH264 | VideoEncoder::FfmpegH264LowLatency => self
                        .stream_handles
                        .push(start_gst_receive_tcp_h264(IP, self.port, a)?),
                    VideoEncoder::FfmpegVp8 => self
                        .stream_handles
                        .push(start_gst_receive_tcp_vp8(IP, self.port, a)?),
                    VideoEncoder::FfmpegVp9 => self
                        .stream_handles
                        .push(start_gst_receive_tcp_vp9(IP, self.port, a)?),
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
        let RtpOutputOptions { video, audio, .. } = &self.options;
        if video.is_none() && audio.is_none() {
            return Err(anyhow!("No stream specified, GStreamer not started!"));
        }
        match video {
            Some(v) => {
                if audio.is_some() {
                    return Err(anyhow!(
                        "Receiving both audio and video on the same port is possible only over TCP!"
                    ));
                }
                match v.encoder {
                    VideoEncoder::FfmpegH264 | VideoEncoder::FfmpegH264LowLatency => self
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
        let RtpOutputOptions {
            ref video,
            ref audio,
            transport_protocol,
            ..
        } = self.options;
        if transport_protocol == TransportProtocol::TcpServer {
            return Err(anyhow!("FFmpeg cannot handle TCP connection."));
        }
        match (video, audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP."
                ));
            }
            (Some(v), None) => match v.encoder {
                VideoEncoder::FfmpegH264 | VideoEncoder::FfmpegH264LowLatency => self
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
            (None, Some(_)) => self
                .stream_handles
                .push(start_ffmpeg_receive_h264(None, Some(self.port))?),
            (None, None) => return Err(anyhow!("No stream specified, ffmpeg not started!")),
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
                error!("Either audio or video has to be specified.");
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
        let options = RtpOutputOptions {
            video: self.video,
            audio: self.audio,
            transport_protocol: self.transport_protocol,
            player: self.player,
        };
        RtpOutput {
            name: self.name,
            port: self.port,
            options,
            stream_handles: vec![],
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
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "resolution": self.resolution.serialize(),
            "encoder": self.encoder.serialize(),
            "initial": {
                "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
            },
        })
    }

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
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
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
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

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
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
        Some(VideoEncoder::FfmpegH264) | Some(VideoEncoder::FfmpegH264LowLatency) => {
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
        Some(VideoEncoder::FfmpegH264) | Some(VideoEncoder::FfmpegH264LowLatency) => {
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
