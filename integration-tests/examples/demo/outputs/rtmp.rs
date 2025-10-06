use anyhow::{Result, anyhow};
use integration_tests::ffmpeg::start_ffmpeg_rtmp_receive;
use serde::{Deserialize, Serialize};
use std::process::Child;

use inquire::{Confirm, Select};
use serde_json::json;
use strum::{Display, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{InputHandle, filter_video_inputs},
    outputs::{AudioEncoder, OutputHandle, VideoEncoder, VideoResolution, scene::Scene},
    players::OutputPlayer,
};

use crate::utils::get_free_port;

#[derive(Debug, Display, Clone)]
pub enum RtmpRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(from = "RtmpOutputSerialize")]
pub struct RtmpOutput {
    #[serde(skip_serializing)]
    name: String,

    #[serde(skip_serializing)]
    url: String,

    #[serde(skip_serializing)]
    port: u16,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,

    #[serde(skip)]
    stream_handles: Vec<Child>,
    player: OutputPlayer,
}

// URL and name fields of `RtmpOutput` depend on the port field which has to be calculated
// dynamically to avoid situation in which ports collide. This struct is required to make it
// possible for name and URL fields to read the port value. JSON is deserialized to this struct and
// remaining fields are determined during conversion
#[derive(Debug, Deserialize)]
pub struct RtmpOutputSerialize {
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
    player: OutputPlayer,
}

impl From<RtmpOutputSerialize> for RtmpOutput {
    fn from(value: RtmpOutputSerialize) -> Self {
        let port = get_free_port();
        let name = format!("rtmp_output_{port}");
        let url = format!("rtmp://127.0.0.1:{port}");
        Self {
            name,
            url,
            port,
            video: value.video,
            audio: value.audio,
            stream_handles: vec![],
            player: value.player,
        }
    }
}

impl RtmpOutput {
    fn start_ffmpeg_recv(&mut self) -> Result<()> {
        let player_handle = start_ffmpeg_rtmp_receive(self.port)?;
        self.stream_handles.push(player_handle);
        Ok(())
    }
}

#[typetag::serde]
impl OutputHandle for RtmpOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        json!({
            "type": "rtmp_client",
            "url": self.url,
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
        match self.player {
            OutputPlayer::Ffmpeg => self.start_ffmpeg_recv(),
            OutputPlayer::Manual => {
                let cmd = format!(
                    "ffmpeg -f flv -listen 1 -i 'rtmp://0.0.0.0:{}' -vcodec copy -f flv - | ffplay -autoexit -f flv -i -",
                    self.port
                );

                println!("Start player: {cmd}");

                loop {
                    let confirmation = Confirm::new("Is player running? [Y/n]")
                        .with_default(true)
                        .prompt()?;
                    if confirmation {
                        return Ok(());
                    }
                }
            }
            _ => Err(anyhow!("Invalid player for RTMP output!")),
        }
    }
}

impl Drop for RtmpOutput {
    fn drop(&mut self) {
        for stream_process in &mut self.stream_handles {
            match stream_process.kill() {
                Ok(_) => {}
                Err(e) => error!("{e}"),
            }
        }
    }
}

pub struct RtmpOutputBuilder {
    name: String,
    url: String,
    port: u16,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
    player: OutputPlayer,
}

impl RtmpOutputBuilder {
    pub fn new() -> Self {
        let port = get_free_port();
        Self {
            name: format!("output_rtmp_{port}"),
            url: format!("rtmp://127.0.0.1:{port}"),
            port,
            video: None,
            audio: None,
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

        builder.prompt_player()
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![
            RtmpRegisterOptions::SetVideoStream,
            RtmpRegisterOptions::Skip,
        ];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(RtmpRegisterOptions::SetVideoStream) => {
                let scene_options = Scene::iter().collect();
                let scene_choice =
                    Select::new("Select scene:", scene_options).prompt_skippable()?;
                let video = match scene_choice {
                    Some(scene) => RtmpOutputVideoOptions {
                        scene,
                        ..Default::default()
                    },
                    None => RtmpOutputVideoOptions::default(),
                };
                Ok(self.with_video(video))
            }
            Some(RtmpRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![
            RtmpRegisterOptions::SetAudioStream,
            RtmpRegisterOptions::Skip,
        ];

        let audio_selection =
            Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

        match audio_selection {
            Some(RtmpRegisterOptions::SetAudioStream) => {
                Ok(self.with_audio(RtmpOutputAudioOptions::default()))
            }
            Some(RtmpRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_player(self) -> Result<Self> {
        let player_options = vec![OutputPlayer::Ffmpeg, OutputPlayer::Manual];
        let player_choice =
            Select::new("Select player (ESC for FFmpeg):", player_options).prompt_skippable()?;
        match player_choice {
            Some(player) => Ok(self.with_player(player)),
            None => Ok(self.with_player(OutputPlayer::Ffmpeg)),
        }
    }

    pub fn with_video(mut self, video: RtmpOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: RtmpOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_player(mut self, player: OutputPlayer) -> Self {
        self.player = player;
        self
    }

    pub fn build(self) -> RtmpOutput {
        RtmpOutput {
            name: self.name,
            url: self.url,
            port: self.port,
            video: self.video,
            audio: self.audio,
            stream_handles: vec![],
            player: self.player,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RtmpOutputVideoOptions {
    root_id: String,
    resolution: VideoResolution,
    encoder: VideoEncoder,
    scene: Scene,
}

impl RtmpOutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);

        json!({
            "resolution": self.resolution.serialize(),
            "encoder" : {
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

impl Default for RtmpOutputVideoOptions {
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
pub struct RtmpOutputAudioOptions {
    encoder: AudioEncoder,
}

impl RtmpOutputAudioOptions {
    pub fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let input_json = inputs
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
                "inputs": input_json,
            }
        })
    }

    pub fn serialize_update(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let input_json = inputs
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
            "inputs": input_json,
        })
    }
}

impl Default for RtmpOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Aac,
        }
    }
}
