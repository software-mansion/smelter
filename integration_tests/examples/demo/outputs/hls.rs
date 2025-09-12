use std::{fs, path::PathBuf, process::Child};

use anyhow::Result;
use inquire::Select;
use integration_tests::{examples::examples_root_dir, ffmpeg::start_ffmpeg_receive_hls};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{filter_video_inputs, InputHandle},
    outputs::{
        scene::Scene, AudioEncoder, OutputHandle, OutputProtocol, VideoEncoder, VideoResolution,
    },
    players::OutputPlayer,
    smelter_state::RunningState,
};

#[derive(Debug, Display, Clone)]
pub enum HlsRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HlsOutput {
    r#type: OutputProtocol,
    name: String,
    path: PathBuf,
    video: Option<HlsOutputVideoOptions>,
    audio: Option<HlsOutputAudioOptions>,
    player: OutputPlayer,

    #[serde(skip)]
    stream_handles: Vec<Child>,
}

impl HlsOutput {
    fn start_ffmpeg_receiver(&mut self) -> Result<()> {
        let stream_handle = start_ffmpeg_receive_hls(&self.path)?;
        self.stream_handles.push(stream_handle);
        Ok(())
    }
}

impl OutputHandle for HlsOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        json!({
            "type": "hls",
            "path": self.path,
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

    fn json_dump(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }

    fn on_before_registration(&mut self) -> Result<()> {
        let dir_path = self.path.parent().unwrap();
        Ok(fs::create_dir(dir_path)?)
    }

    fn on_after_registration(&mut self) -> Result<()> {
        match self.player {
            OutputPlayer::Ffmpeg => self.start_ffmpeg_receiver(),
            OutputPlayer::Manual => {
                let cmd = format!("ffplay -i {}", self.path.to_str().unwrap());
                println!("Run this command AFTER the start request:");
                println!("{cmd}");
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}

impl Drop for HlsOutput {
    fn drop(&mut self) {
        let dir_path = self.path.parent().unwrap();
        fs::remove_dir_all(dir_path).unwrap();

        for stream in &mut self.stream_handles {
            if let Err(e) = stream.kill() {
                error!("{e}");
            }
        }
    }
}

pub struct HlsOutputBuilder {
    name: String,
    video: Option<HlsOutputVideoOptions>,
    audio: Option<HlsOutputAudioOptions>,
    player: OutputPlayer,
}

impl HlsOutputBuilder {
    pub fn new() -> Self {
        let suffix = rand::thread_rng().next_u32();
        let name = format!("hls_output_{suffix}");
        Self {
            name,
            video: None,
            audio: None,
            player: OutputPlayer::Manual,
        }
    }

    pub fn prompt(self, running_state: RunningState) -> Result<Self> {
        let mut builder = self;

        loop {
            builder = builder.prompt_video()?.prompt_audio()?;

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either video or audio has to be specified.");
            } else {
                break;
            }
        }

        builder.prompt_player(running_state)
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![HlsRegisterOptions::SetVideoStream, HlsRegisterOptions::Skip];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(HlsRegisterOptions::SetVideoStream) => {
                let scene_options = Scene::iter().collect();
                let scene_choice =
                    Select::new("Select scene:", scene_options).prompt_skippable()?;
                let video = match scene_choice {
                    Some(scene) => HlsOutputVideoOptions {
                        scene,
                        ..Default::default()
                    },
                    None => HlsOutputVideoOptions::default(),
                };
                Ok(self.with_video(video))
            }
            Some(HlsRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![HlsRegisterOptions::SetAudioStream, HlsRegisterOptions::Skip];
        let audio_selection =
            Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

        match audio_selection {
            Some(HlsRegisterOptions::SetAudioStream) => {
                Ok(self.with_audio(HlsOutputAudioOptions::default()))
            }
            Some(HlsRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_player(self, running_state: RunningState) -> Result<Self> {
        let (player_options, default_player) = match running_state {
            RunningState::Running => (
                vec![OutputPlayer::Ffmpeg, OutputPlayer::Manual],
                OutputPlayer::Ffmpeg,
            ),
            RunningState::Idle => (vec![OutputPlayer::Manual], OutputPlayer::Manual),
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

    pub fn with_video(mut self, video: HlsOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: HlsOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_player(mut self, player: OutputPlayer) -> Self {
        self.player = player;
        self
    }

    pub fn build(self) -> HlsOutput {
        let path = examples_root_dir().join(&self.name).join("index.m3u8");
        HlsOutput {
            r#type: OutputProtocol::Hls,
            name: self.name,
            path,
            video: self.video,
            audio: self.audio,
            player: self.player,
            stream_handles: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HlsOutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
    root_id: String,
    scene: Scene,
}

impl HlsOutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "resolution": self.resolution.serialize(),
            "encoder": {
                "type": self.encoder.to_string(),
            },
            "initial": {
                "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
            },
        })
    }

    pub fn serialize_update(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
        })
    }
}

impl Default for HlsOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        let root_id = "root".to_string();
        Self {
            resolution,
            encoder: VideoEncoder::FfmpegH264,
            root_id,
            scene: Scene::Tiles,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HlsOutputAudioOptions {
    encoder: AudioEncoder,
}

impl HlsOutputAudioOptions {
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

impl Default for HlsOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Aac,
        }
    }
}
