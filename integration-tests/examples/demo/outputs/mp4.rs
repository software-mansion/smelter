use std::{env, path::PathBuf};

use anyhow::Result;
use inquire::{Select, Text};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, IntoEnumIterator};
use tracing::error;

use crate::{
    autocompletion::FilePathCompleter,
    inputs::{InputHandle, filter_video_inputs},
    outputs::{AudioEncoder, VideoEncoder, VideoResolution, scene::Scene},
    utils::resolve_path,
};
const MP4_OUTPUT_PATH: &str = "MP4_OUTPUT_PATH";

#[derive(Debug, Display, Clone)]
pub enum Mp4RegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "Mp4OutputOptions")]
#[serde(into = "Mp4OutputOptions")]
pub struct Mp4Output {
    name: String,
    options: Mp4OutputOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mp4OutputOptions {
    path: PathBuf,
    video: Option<Mp4OutputVideoOptions>,
    audio: Option<Mp4OutputAudioOptions>,
}

impl From<Mp4OutputOptions> for Mp4Output {
    fn from(value: Mp4OutputOptions) -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("mp4_output_{suffix}");
        Self {
            name,
            options: value,
        }
    }
}

impl From<Mp4Output> for Mp4OutputOptions {
    fn from(value: Mp4Output) -> Self {
        value.options
    }
}

impl Mp4Output {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let Mp4OutputOptions { path, video, audio } = &self.options;
        json!({
            "type": "mp4",
            "path": path,
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
}

pub struct Mp4OutputBuilder {
    name: String,
    path: Option<PathBuf>,
    video: Option<Mp4OutputVideoOptions>,
    audio: Option<Mp4OutputAudioOptions>,
}

impl Mp4OutputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("mp4_output_{suffix}");
        Self {
            name,
            path: None,
            video: None,
            audio: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        builder = builder.prompt_path()?;

        loop {
            builder = builder.prompt_video()?.prompt_audio()?;

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either video or audio has to be specified.");
            } else {
                break;
            }
        }

        Ok(builder)
    }

    fn prompt_path(self) -> Result<Self> {
        let env_path = env::var(MP4_OUTPUT_PATH).unwrap_or_default();

        let default_path = env::current_dir().unwrap().join("example_output.mp4");

        loop {
            let path_output = Text::new("Output path (ESC for default):")
                .with_autocomplete(FilePathCompleter::default())
                .with_initial_value(&env_path)
                .with_default(default_path.to_str().unwrap())
                .prompt_skippable()?;

            match path_output {
                Some(path) if !path.trim().is_empty() => {
                    let path = resolve_path(path.into())?;
                    let parent = path.parent();
                    match parent {
                        Some(p) if p.exists() => break Ok(self.with_path(path)),
                        Some(_) | None => error!("Path is not valid"),
                    }
                }
                Some(_) | None => break Ok(self.with_path(default_path)),
            }
        }
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![Mp4RegisterOptions::SetVideoStream, Mp4RegisterOptions::Skip];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(Mp4RegisterOptions::SetVideoStream) => {
                let scene_options = Scene::iter().collect();
                let scene_choice =
                    Select::new("Select scene:", scene_options).prompt_skippable()?;
                let video = match scene_choice {
                    Some(scene) => Mp4OutputVideoOptions {
                        scene,
                        ..Default::default()
                    },
                    None => Mp4OutputVideoOptions::default(),
                };
                Ok(self.with_video(video))
            }
            Some(Mp4RegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![Mp4RegisterOptions::SetAudioStream, Mp4RegisterOptions::Skip];
        let audio_selection =
            Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

        match audio_selection {
            Some(Mp4RegisterOptions::SetAudioStream) => {
                Ok(self.with_audio(Mp4OutputAudioOptions::default()))
            }
            Some(Mp4RegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_video(mut self, video: Mp4OutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: Mp4OutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn build(self) -> Mp4Output {
        let options = Mp4OutputOptions {
            path: self.path.unwrap(),
            video: self.video,
            audio: self.audio,
        };
        Mp4Output {
            name: self.name,
            options,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mp4OutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
    root_id: String,
    scene: Scene,
}

impl Mp4OutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
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

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
        })
    }
}

impl Default for Mp4OutputVideoOptions {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mp4OutputAudioOptions {
    encoder: AudioEncoder,
}

impl Mp4OutputAudioOptions {
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

impl Default for Mp4OutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Aac,
        }
    }
}
