use std::{
    env,
    path::{Path, PathBuf},
    process::Child,
};

use anyhow::{anyhow, Result};
use inquire::{Select, Text};
use integration_tests::{
    assets::{
        BUNNY_H264_PATH, BUNNY_H264_URL, BUNNY_VP8_PATH, BUNNY_VP8_URL, BUNNY_VP9_PATH,
        BUNNY_VP9_URL,
    },
    examples::{download_asset, examples_root_dir, AssetData, TestSample},
    ffmpeg::{start_ffmpeg_send, start_ffmpeg_send_from_file},
    gstreamer::{
        start_gst_send_from_file_tcp, start_gst_send_from_file_udp, start_gst_send_tcp,
        start_gst_send_udp,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    autocompletion::FilePathCompleter,
    inputs::{AudioDecoder, InputHandler, VideoDecoder},
    players::InputPlayer,
    smelter_state::TransportProtocol,
    utils::resolve_path,
    IP,
};

use crate::utils::get_free_port;

const RTP_INPUT_PATH: &str = "RTP_INPUT_PATH";

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
#[serde(from = "RtpInputSerialize")]
pub struct RtpInput {
    name: String,
    port: u16,
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: TransportProtocol,
    path: Option<PathBuf>,

    #[serde(skip)]
    stream_handles: Vec<Child>,
    player: InputPlayer,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RtpInputSerialize {
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: TransportProtocol,
    path: Option<PathBuf>,
    player: InputPlayer,
}

impl From<RtpInputSerialize> for RtpInput {
    fn from(value: RtpInputSerialize) -> Self {
        let port = get_free_port();
        let name = format!("rtp_input_{}_{port}", value.transport_protocol);
        Self {
            name,
            port,
            video: value.video,
            audio: value.audio,
            transport_protocol: value.transport_protocol,
            path: value.path,
            stream_handles: vec![],
            player: value.player,
        }
    }
}

impl From<&RtpInput> for RtpInputSerialize {
    fn from(value: &RtpInput) -> Self {
        Self {
            video: value.video.clone(),
            audio: value.audio.clone(),
            transport_protocol: value.transport_protocol,
            path: value.path.clone(),
            player: value.player,
        }
    }
}

impl RtpInput {
    fn test_sample(&self) -> TestSample {
        match &self.video {
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegH264,
            })
            | None => TestSample::BigBuckBunnyH264Opus,
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegVp8,
            }) => TestSample::BigBuckBunnyVP8Opus,
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegVp9,
            }) => TestSample::BigBuckBunnyVP9Opus,
            _ => unreachable!(),
        }
    }

    fn download_asset(&self) -> Result<()> {
        let asset = match self.video {
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegH264,
            })
            | None => AssetData {
                url: BUNNY_H264_URL.to_string(),
                path: examples_root_dir().join(BUNNY_H264_PATH),
            },
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegVp8,
            }) => AssetData {
                url: BUNNY_VP8_URL.to_string(),
                path: examples_root_dir().join(BUNNY_VP8_PATH),
            },
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegVp9,
            }) => AssetData {
                url: BUNNY_VP9_URL.to_string(),
                path: examples_root_dir().join(BUNNY_VP9_PATH),
            },
            _ => unreachable!(),
        };

        download_asset(&asset)
    }

    fn gstreamer_transmit_tcp(&mut self) -> Result<()> {
        let video_port = self.video.as_ref().map(|_| self.port);
        let audio_port = self.audio.as_ref().map(|_| self.port);
        let video_codec = self.video.as_ref().map(|v| v.decoder.into());
        let handle = match &self.path {
            Some(path) => {
                start_gst_send_from_file_tcp(IP, video_port, audio_port, path.clone(), video_codec)?
            }
            None => {
                if self.video.is_some() {
                    self.download_asset()?;
                }
                start_gst_send_tcp(IP, video_port, audio_port, self.test_sample())?
            }
        };
        self.stream_handles.push(handle);

        Ok(())
    }

    fn gstreamer_transmit_udp(&mut self) -> Result<()> {
        let video_port = self.video.as_ref().map(|_| self.port);
        let audio_port = self.audio.as_ref().map(|_| self.port);
        let video_codec = self.video.as_ref().map(|v| v.decoder.into());
        let handle = match &self.path {
            Some(path) => {
                start_gst_send_from_file_udp(IP, video_port, audio_port, path.clone(), video_codec)?
            }
            None => {
                if self.video.is_some() {
                    self.download_asset()?;
                }
                start_gst_send_udp(IP, video_port, audio_port, self.test_sample())?
            }
        };
        self.stream_handles.push(handle);
        Ok(())
    }

    fn ffmpeg_transmit(&mut self) -> Result<()> {
        let (video_handle, audio_handle) = match (&self.video, &self.audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP."
                ))
            }
            (Some(video), None) => {
                let video_codec = video.decoder.into();
                match &self.path {
                    Some(path) => start_ffmpeg_send_from_file(
                        IP,
                        Some(self.port),
                        None,
                        path.clone(),
                        Some(video_codec),
                    )?,
                    None => {
                        self.download_asset()?;
                        start_ffmpeg_send(IP, Some(self.port), None, self.test_sample())?
                    }
                }
            }
            (None, Some(_audio)) => match &self.path {
                Some(path) => {
                    start_ffmpeg_send_from_file(IP, None, Some(self.port), path.clone(), None)?
                }
                None => {
                    self.download_asset()?;
                    start_ffmpeg_send(IP, None, Some(self.port), self.test_sample())?
                }
            },
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
        match self.player {
            InputPlayer::FfmpegTransmitter => self.ffmpeg_transmit(),
            InputPlayer::GstreamerTransmitter => self.gstreamer_transmit_udp(),
            InputPlayer::Manual => {
                let video_codec = self.video.as_ref().map(|opts| opts.decoder);
                let has_audio = self.audio.is_some();
                let file_path = match &self.path {
                    Some(p) => p,
                    None => &examples_root_dir().join(match video_codec {
                        Some(VideoDecoder::FfmpegVp9) => BUNNY_VP9_PATH,
                        Some(VideoDecoder::FfmpegVp8) => BUNNY_VP8_PATH,
                        _ => BUNNY_H264_PATH,
                    }),
                };
                let cmd = build_gst_send_udp_cmd(video_codec, has_audio, self.port, file_path);
                match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => {
                        println!("Start streaming H264 encoded video and OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    (Some(_), None) => {
                        println!("Start streaming H264 encoded video:");
                        println!("{cmd}");
                        println!();
                    }
                    (None, Some(_)) => {
                        println!("Start streaming OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    _ => unreachable!(),
                }
                Ok(())
            }
        }
    }

    fn on_after_registration_tcp(&mut self) -> Result<()> {
        match self.player {
            InputPlayer::GstreamerTransmitter => self.gstreamer_transmit_tcp(),
            InputPlayer::Manual => {
                let video_codec = self.video.as_ref().map(|opts| opts.decoder);
                let has_audio = self.audio.is_some();
                let file_path = match &self.path {
                    Some(p) => p,
                    None => &examples_root_dir().join(match video_codec {
                        Some(VideoDecoder::FfmpegVp9) => BUNNY_VP9_PATH,
                        Some(VideoDecoder::FfmpegVp8) => BUNNY_VP8_PATH,
                        _ => BUNNY_H264_PATH,
                    }),
                };
                let cmd = build_gst_send_tcp_cmd(video_codec, has_audio, self.port, file_path);
                match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => {
                        println!("Start streaming H264 encoded video and OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    (Some(_), None) => {
                        println!("Start streaming H264 encoded video:");
                        println!("{cmd}");
                        println!();
                    }
                    (None, Some(_)) => {
                        println!("Start streaming OPUS encoded audio:");
                        println!("{cmd}");
                        println!();
                    }
                    _ => unreachable!(),
                }
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}

#[typetag::serde]
impl InputHandler for RtpInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self) -> serde_json::Value {
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "transport_protocol": self.transport_protocol.to_string(),
            "video": self.video.as_ref().map(|v| v.serialize()),
            "audio": self.audio.as_ref().map(|a| a.serialize()),
        })
    }

    fn json_dump(&self) -> Result<serde_json::Value> {
        let rtp_input_serde: RtpInputSerialize = self.into();
        Ok(serde_json::to_value(rtp_input_serde)?)
    }

    fn has_video(&self) -> bool {
        self.video.is_some()
    }

    fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    fn on_after_registration(&mut self) -> Result<()> {
        match self.transport_protocol {
            TransportProtocol::TcpServer => self.on_after_registration_tcp(),
            TransportProtocol::Udp => self.on_after_registration_udp(),
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
    path: Option<PathBuf>,
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
            path: None,
            player: InputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        builder = builder.prompt_path()?;

        let audio_options = vec![RtpRegisterOptions::SetAudioStream, RtpRegisterOptions::Skip];
        loop {
            builder = builder.prompt_video()?;
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

        builder = builder.prompt_player()?;

        Ok(builder)
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![RtpRegisterOptions::SetVideoStream, RtpRegisterOptions::Skip];

        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(RtpRegisterOptions::SetVideoStream) => {
                let codec_options = VideoDecoder::iter()
                    .filter(|dec| *dec != VideoDecoder::Any)
                    .collect();

                let codec_choice =
                    Select::new("Select video codec to test:", codec_options).prompt_skippable()?;

                match codec_choice {
                    Some(codec) => Ok(self.with_video(RtpInputVideoOptions { decoder: codec })),
                    None => Ok(self.with_video(RtpInputVideoOptions {
                        decoder: VideoDecoder::FfmpegH264,
                    })),
                }
            }
            Some(_) | None => Ok(self),
        }
    }

    fn prompt_path(self) -> Result<Self> {
        let env_path = env::var(RTP_INPUT_PATH).unwrap_or_default();
        let default_path = examples_root_dir().join(BUNNY_H264_PATH);

        loop {
            let path_input = Text::new(&format!(
                "Input path (ESC for {}):",
                default_path.to_str().unwrap(),
            ))
            .with_autocomplete(FilePathCompleter::default())
            .with_initial_value(&env_path)
            .prompt_skippable()?;

            match path_input {
                Some(path) if !path.trim().is_empty() => {
                    let path = resolve_path(path.into())?;
                    if path.exists() {
                        break Ok(self.with_path(path));
                    } else {
                        error!("Path is not valid");
                    }
                }
                Some(_) | None => break Ok(self),
            }
        }
    }

    fn prompt_player(self) -> Result<Self> {
        match self.transport_protocol {
            Some(TransportProtocol::Udp) | None => {
                let player_options = match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => {
                        vec![InputPlayer::GstreamerTransmitter, InputPlayer::Manual]
                    }
                    _ => InputPlayer::iter().collect(),
                };

                let player_selection = Select::new("Select player:", player_options).prompt()?;
                Ok(self.with_player(player_selection))
            }
            Some(TransportProtocol::TcpServer) => {
                let player_options = vec![InputPlayer::GstreamerTransmitter, InputPlayer::Manual];
                let player_selection = Select::new("Select player:", player_options).prompt()?;
                Ok(self.with_player(player_selection))
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

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_player(mut self, player: InputPlayer) -> Self {
        self.player = player;
        self
    }

    pub fn build(self) -> RtpInput {
        RtpInput {
            name: self.name,
            port: self.port,
            video: self.video,
            audio: self.audio,
            path: self.path,
            transport_protocol: self.transport_protocol.unwrap_or(TransportProtocol::Udp),
            stream_handles: vec![],
            player: self.player,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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

fn build_gst_send_tcp_cmd(
    video_codec: Option<VideoDecoder>,
    has_audio: bool,
    port: u16,
    file_path: &Path,
) -> String {
    if !has_audio && video_codec.is_none() {
        return String::new();
    }

    let demuxer = match video_codec {
        Some(VideoDecoder::FfmpegVp8) | Some(VideoDecoder::FfmpegVp9) => "matroskademux",
        Some(VideoDecoder::FfmpegH264) => "qtdemux",
        _ => unreachable!(),
    };

    let base_cmd = format!(
        "gst-launch-1.0 -v filesrc location={} ! {demuxer} name=demux ",
        file_path.to_str().unwrap(),
    );

    let video_cmd = match video_codec {
        Some(VideoDecoder::FfmpegH264) =>  format!("demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={port} "),
        Some(VideoDecoder::FfmpegVp8) => format!("demux.video_0 ! queue ! rtpvp8pay mtu=1200 picture-id-mode=2 !  application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={port} "),
        Some(VideoDecoder::FfmpegVp9) => format!("demux.video_0 ! queue ! rtpvp9pay mtu=1200 picture-id-mode=2 !  application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={port} "),
        None => String::new(),
        _ => unreachable!(),
    };

    let audio_cmd = if has_audio {
        format!("demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 !  rtpstreampay ! tcpclientsink host='127.0.0.1' port={port}")
    } else {
        String::new()
    };

    base_cmd + &video_cmd + &audio_cmd
}

fn build_gst_send_udp_cmd(
    video_codec: Option<VideoDecoder>,
    has_audio: bool,
    port: u16,
    file_path: &Path,
) -> String {
    if !has_audio && video_codec.is_none() {
        return String::new();
    }

    let demuxer = match video_codec {
        Some(VideoDecoder::FfmpegVp8) | Some(VideoDecoder::FfmpegVp9) => "matroskademux",
        Some(VideoDecoder::FfmpegH264) => "qtdemux",
        _ => unreachable!(),
    };

    let base_cmd = format!(
        "gst-launch-1.0 -v filesrc location={} ! {demuxer} name=demux ",
        file_path.to_str().unwrap(),
    );

    let video_cmd = match video_codec {
        Some(VideoDecoder::FfmpegH264) =>  format!("demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96  ! udpsink host='127.0.0.1' port={port} "),
        Some(VideoDecoder::FfmpegVp8) => format!("demux.video_0 ! queue ! rtpvp8pay mtu=1200 picture-id-mode=2 !  application/x-rtp,payload=96  ! udpsink host='127.0.0.1' port={port} "),
        Some(VideoDecoder::FfmpegVp9) => format!("demux.video_0 ! queue ! rtpvp9pay picture-id-mode=2 mtu=1200 ! application/x-rtp,payload=96 ! udpsink host='127.0.0.1' port={port} "),
        None => String::new(),
        _ => unreachable!(),
    };

    let audio_cmd = if has_audio {
        format!("demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! udpsink host='127.0.0.1' port={port}")
    } else {
        String::new()
    };

    base_cmd + &video_cmd + &audio_cmd
}
