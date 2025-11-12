use std::{
    env,
    path::{Path, PathBuf},
    process::Child,
};

use anyhow::{Result, anyhow};
use inquire::{Select, Text};
use integration_tests::{
    assets::{
        BUNNY_H264_PATH, BUNNY_H264_URL, BUNNY_VP8_PATH, BUNNY_VP8_URL, BUNNY_VP9_PATH,
        BUNNY_VP9_URL,
    },
    examples::{AssetData, TestSample, download_asset},
    ffmpeg::{start_ffmpeg_send, start_ffmpeg_send_from_file},
    gstreamer::{
        start_gst_send_from_file_tcp, start_gst_send_from_file_udp, start_gst_send_tcp,
        start_gst_send_udp,
    },
    paths::integration_tests_root,
};
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    IP,
    autocompletion::FilePathCompleter,
    inputs::{AudioDecoder, VideoDecoder},
    players::InputPlayer,
    utils::resolve_path,
};

use crate::utils::get_free_port;

const RTP_INPUT_PATH: &str = "RTP_INPUT_PATH";

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
#[serde(from = "RtpInputOptions")]
pub struct RtpInput {
    pub name: String,
    port: u16,
    options: RtpInputOptions,
    stream_handles: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtpInputOptions {
    video: Option<RtpInputVideoOptions>,
    audio: Option<RtpInputAudioOptions>,
    transport_protocol: TransportProtocol,
    path: Option<PathBuf>,
    player: InputPlayer,
}

impl Serialize for RtpInput {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("RtpInput", 5)?;
        state.serialize_field("video", &self.options.video)?;
        state.serialize_field("audio", &self.options.audio)?;
        state.serialize_field("transport_protocol", &self.options.transport_protocol)?;
        state.serialize_field("path", &self.options.path)?;
        state.serialize_field("player", &self.options.player)?;
        state.end()
    }
}

impl From<RtpInputOptions> for RtpInput {
    fn from(value: RtpInputOptions) -> Self {
        let port = get_free_port();
        let name = format!("rtp_input_{}_{port}", value.transport_protocol);
        Self {
            name,
            port,
            options: value,
            stream_handles: vec![],
        }
    }
}

impl RtpInput {
    pub fn serialize_register(&self) -> serde_json::Value {
        let RtpInputOptions {
            ref video,
            ref audio,
            transport_protocol,
            ..
        } = self.options;
        json!({
            "type": "rtp_stream",
            "port": self.port,
            "transport_protocol": transport_protocol.to_string(),
            "video": video.as_ref().map(|v| v.serialize()),
            "audio": audio.as_ref().map(|a| a.serialize()),
        })
    }

    pub fn has_video(&self) -> bool {
        self.options.video.is_some()
    }

    pub fn has_audio(&self) -> bool {
        self.options.audio.is_some()
    }

    pub fn on_after_registration(&mut self) -> Result<()> {
        match self.options.transport_protocol {
            TransportProtocol::TcpServer => self.on_after_registration_tcp(),
            TransportProtocol::Udp => self.on_after_registration_udp(),
        }
    }

    fn test_sample(&self) -> TestSample {
        match &self.options.video {
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegH264,
            })
            | Some(RtpInputVideoOptions {
                decoder: VideoDecoder::VulkanH264,
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
        let asset = match self.options.video {
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegH264,
            })
            | Some(RtpInputVideoOptions {
                decoder: VideoDecoder::VulkanH264,
            })
            | None => AssetData {
                url: BUNNY_H264_URL.to_string(),
                path: integration_tests_root().join(BUNNY_H264_PATH),
            },
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegVp8,
            }) => AssetData {
                url: BUNNY_VP8_URL.to_string(),
                path: integration_tests_root().join(BUNNY_VP8_PATH),
            },
            Some(RtpInputVideoOptions {
                decoder: VideoDecoder::FfmpegVp9,
            }) => AssetData {
                url: BUNNY_VP9_URL.to_string(),
                path: integration_tests_root().join(BUNNY_VP9_PATH),
            },
            _ => unreachable!(),
        };

        download_asset(&asset)
    }

    fn gstreamer_transmit_tcp(&mut self) -> Result<()> {
        let RtpInputOptions {
            video, audio, path, ..
        } = &self.options;
        let video_port = video.as_ref().map(|_| self.port);
        let audio_port = audio.as_ref().map(|_| self.port);
        let video_codec = video.as_ref().map(|v| v.decoder.into());
        let handle = match path {
            Some(path) => {
                start_gst_send_from_file_tcp(IP, video_port, audio_port, path.clone(), video_codec)?
            }
            None => {
                if video.is_some() {
                    self.download_asset()?;
                }
                start_gst_send_tcp(IP, video_port, audio_port, self.test_sample())?
            }
        };
        self.stream_handles.push(handle);

        Ok(())
    }

    fn gstreamer_transmit_udp(&mut self) -> Result<()> {
        let RtpInputOptions {
            video, audio, path, ..
        } = &self.options;
        let video_port = video.as_ref().map(|_| self.port);
        let audio_port = audio.as_ref().map(|_| self.port);
        let video_codec = video.as_ref().map(|v| v.decoder.into());
        let handle = match path {
            Some(path) => {
                start_gst_send_from_file_udp(IP, video_port, audio_port, path.clone(), video_codec)?
            }
            None => {
                if video.is_some() {
                    self.download_asset()?;
                }
                start_gst_send_udp(IP, video_port, audio_port, self.test_sample())?
            }
        };
        self.stream_handles.push(handle);
        Ok(())
    }

    fn ffmpeg_transmit(&mut self) -> Result<()> {
        let RtpInputOptions {
            video, audio, path, ..
        } = &self.options;
        let (video_handle, audio_handle) = match (video, audio) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "FFmpeg can't handle both audio and video on a single port over RTP."
                ));
            }
            (Some(v), None) => {
                let video_codec = v.decoder.into();
                match path {
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
            (None, Some(_audio)) => match path {
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
        let RtpInputOptions {
            ref video,
            ref audio,
            ref path,
            player,
            ..
        } = self.options;
        match player {
            InputPlayer::Ffmpeg => self.ffmpeg_transmit(),
            InputPlayer::Gstreamer => self.gstreamer_transmit_udp(),
            InputPlayer::Manual => {
                let video_codec = video.as_ref().map(|opts| opts.decoder);
                let has_audio = audio.is_some();
                let file_path = match path {
                    Some(p) => p,
                    None => &integration_tests_root().join(match video_codec {
                        Some(VideoDecoder::FfmpegVp9) => BUNNY_VP9_PATH,
                        Some(VideoDecoder::FfmpegVp8) => BUNNY_VP8_PATH,
                        _ => BUNNY_H264_PATH,
                    }),
                };
                let cmd = build_gst_send_udp_cmd(video_codec, has_audio, self.port, file_path);
                match (video, audio) {
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
        let RtpInputOptions {
            ref video,
            ref audio,
            ref path,
            player,
            ..
        } = self.options;
        match player {
            InputPlayer::Gstreamer => self.gstreamer_transmit_tcp(),
            InputPlayer::Manual => {
                let video_codec = video.as_ref().map(|opts| opts.decoder);
                let has_audio = audio.is_some();
                let file_path = match path {
                    Some(p) => p,
                    None => &integration_tests_root().join(match video_codec {
                        Some(VideoDecoder::FfmpegVp9) => BUNNY_VP9_PATH,
                        Some(VideoDecoder::FfmpegVp8) => BUNNY_VP8_PATH,
                        _ => BUNNY_H264_PATH,
                    }),
                };
                let cmd = build_gst_send_tcp_cmd(video_codec, has_audio, self.port, file_path);
                match (video, audio) {
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
        let name = format!("input_rtp_udp_{port}");
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

        loop {
            builder = builder.prompt_video()?.prompt_audio()?;

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either audio or video stream has to be specified.");
            } else {
                break;
            }
        }

        builder.prompt_transport_protocol()?.prompt_player()
    }

    fn prompt_path(self) -> Result<Self> {
        let env_path = env::var(RTP_INPUT_PATH).unwrap_or_default();
        let default_path = integration_tests_root().join(BUNNY_H264_PATH);

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

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![RtpRegisterOptions::SetAudioStream, RtpRegisterOptions::Skip];
        let audio_selection =
            Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

        match audio_selection {
            Some(RtpRegisterOptions::SetAudioStream) => {
                Ok(self.with_audio(RtpInputAudioOptions::default()))
            }
            Some(RtpRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_transport_protocol(self) -> Result<Self> {
        let transport_options = TransportProtocol::iter().collect();
        let transport_selection = Select::new(
            "Select transport protocol (ESC for udp):",
            transport_options,
        )
        .prompt_skippable()?;

        match transport_selection {
            Some(prot) => Ok(self.with_transport_protocol(prot)),
            None => Ok(self),
        }
    }

    fn prompt_player(self) -> Result<Self> {
        match self.transport_protocol {
            Some(TransportProtocol::Udp) | None => {
                let player_options = match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => {
                        vec![InputPlayer::Gstreamer, InputPlayer::Manual]
                    }
                    _ => InputPlayer::iter().collect(),
                };

                let player_selection =
                    Select::new("Select player (ESC for GStreamer):", player_options)
                        .prompt_skippable()?;
                match player_selection {
                    Some(player) => Ok(self.with_player(player)),
                    None => Ok(self.with_player(InputPlayer::Gstreamer)),
                }
            }
            Some(TransportProtocol::TcpServer) => {
                let (player_options, default_player) = match (&self.video, &self.audio) {
                    (Some(_), Some(_)) => (vec![InputPlayer::Manual], InputPlayer::Manual),
                    _ => (
                        vec![InputPlayer::Gstreamer, InputPlayer::Manual],
                        InputPlayer::Gstreamer,
                    ),
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
        let options = RtpInputOptions {
            path: self.path,
            video: self.video,
            audio: self.audio,
            transport_protocol: self.transport_protocol.unwrap_or(TransportProtocol::Udp),
            player: self.player,
        };
        RtpInput {
            name: self.name,
            port: self.port,
            options,
            stream_handles: vec![],
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
        Some(VideoDecoder::FfmpegH264) => format!(
            "demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={port} "
        ),
        Some(VideoDecoder::FfmpegVp8) => format!(
            "demux.video_0 ! queue ! rtpvp8pay mtu=1200 picture-id-mode=2 !  application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={port} "
        ),
        Some(VideoDecoder::FfmpegVp9) => format!(
            "demux.video_0 ! queue ! rtpvp9pay mtu=1200 picture-id-mode=2 !  application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host='127.0.0.1' port={port} "
        ),
        None => String::new(),
        _ => unreachable!(),
    };

    let audio_cmd = if has_audio {
        format!(
            "demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 !  rtpstreampay ! tcpclientsink host='127.0.0.1' port={port}"
        )
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
        Some(VideoDecoder::FfmpegH264) => format!(
            "demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96  ! udpsink host='127.0.0.1' port={port} "
        ),
        Some(VideoDecoder::FfmpegVp8) => format!(
            "demux.video_0 ! queue ! rtpvp8pay mtu=1200 picture-id-mode=2 !  application/x-rtp,payload=96  ! udpsink host='127.0.0.1' port={port} "
        ),
        Some(VideoDecoder::FfmpegVp9) => format!(
            "demux.video_0 ! queue ! rtpvp9pay picture-id-mode=2 mtu=1200 ! application/x-rtp,payload=96 ! udpsink host='127.0.0.1' port={port} "
        ),
        None => String::new(),
        _ => unreachable!(),
    };

    let audio_cmd = if has_audio {
        format!(
            "demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! udpsink host='127.0.0.1' port={port}"
        )
    } else {
        String::new()
    };

    base_cmd + &video_cmd + &audio_cmd
}
