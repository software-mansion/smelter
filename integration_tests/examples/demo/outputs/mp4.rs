use std::env;

use anyhow::Result;
use inquire::{Select, Text};
use integration_tests::examples::examples_root_dir;
use rand::RngCore;
use serde_json::json;
use strum::Display;
use tracing::error;

use crate::{
    outputs::{AudioEncoder, OutputHandler, VideoEncoder, VideoResolution},
    players::OutputPlayer,
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

#[derive(Debug)]
pub struct Mp4Output {
    name: String,
    video: Option<Mp4OutputVideoOptions>,
    audio: Option<Mp4OutputAudioOptions>,
}

impl OutputHandler for Mp4Output {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value {
        json!({
           "video": self.video.as_ref().map(|v| v.serialize_update(inputs, &self.name)),
           "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }
}

pub struct Mp4OutputBuilder {
    name: String,
    path: Option<String>,
    video: Option<Mp4OutputVideoOptions>,
    audio: Option<Mp4OutputAudioOptions>,
}

impl Mp4OutputBuilder {
    pub fn new() -> Self {
        let suffix = rand::thread_rng().next_u32();
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
        let env_path = env::var(MP4_OUTPUT_PATH).unwrap_or_default();

        let default_path = examples_root_dir().join("example_output.mp4");
        let path_output =
            Text::new("Output path (absolute or relative to 'smelter/integration_tests'):")
                .with_initial_value(&env_path)
                .with_default(default_path.to_str().unwrap())
                .prompt()?;

        builder = builder.with_path(path_output);

        let video_options = vec![Mp4RegisterOptions::SetVideoStream, Mp4RegisterOptions::Skip];
        let audio_options = vec![Mp4RegisterOptions::SetAudioStream, Mp4RegisterOptions::Skip];

        loop {
            let video_selection =
                Select::new("Set video stream?", video_options.clone()).prompt_skippable()?;

            builder = match video_selection {
                Some(Mp4RegisterOptions::SetVideoStream) => {
                    builder.with_video(Mp4OutputVideoOptions::default())
                }
                Some(Mp4RegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            let audio_selection =
                Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(Mp4RegisterOptions::SetAudioStream) => {
                    builder.with_audio(Mp4OutputAudioOptions::default())
                }
                Some(Mp4RegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either video or audio has to be specified.");
            } else {
                break;
            }
        }

        Ok(builder)
    }

    pub fn with_path(mut self, path: String) -> Self {
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

    fn serialize(&self, inputs: &[&str]) -> serde_json::Value {
        json!({
            "type": "mp4",
            "path": self.path.as_ref().unwrap(),
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs, &self.name)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    pub fn build(self, inputs: &[&str]) -> (Mp4Output, serde_json::Value, OutputPlayer) {
        let register_request = self.serialize(inputs);

        let mp4_output = Mp4Output {
            name: self.name,
            video: self.video,
            audio: self.audio,
        };

        (mp4_output, register_request, OutputPlayer::Manual)
    }
}

#[derive(Debug)]
pub struct Mp4OutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
    root_id: String,
}

impl Mp4OutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[&str], output_name: &str) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_name| {
                let id = format!("{input_name}_{output_name}");
                json!({
                    "type": "input_stream",
                    "id": id,
                    "input_id": input_name,
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
                    "id": self.root_id,
                    "transition": {
                        "duration_ms": 500,
                    },
                    "children": input_json,
                },
            },
        })
    }

    pub fn serialize_update(&self, inputs: &[&str], output_name: &str) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_name| {
                let id = format!("{input_name}_{output_name}");
                json!({
                    "type": "input_stream",
                    "id": id,
                    "input_id": input_name,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "root": {
                "type": "tiles",
                "id": self.root_id,
                "transition": {
                    "duration_ms": 500,
                },
                "children": input_json,
            }
        })
    }
}

impl Default for Mp4OutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        let suffix = rand::thread_rng().next_u32();
        let root_id = format!("tiles_{suffix}");
        Self {
            resolution,
            encoder: VideoEncoder::FfmpegH264,
            root_id,
        }
    }
}

#[derive(Debug)]
pub struct Mp4OutputAudioOptions {
    encoder: AudioEncoder,
}

impl Mp4OutputAudioOptions {
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

impl Default for Mp4OutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Aac,
        }
    }
}
