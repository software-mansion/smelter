use std::sync::{
    OnceLock,
    atomic::{AtomicU32, Ordering},
};

use anyhow::Result;
use inquire::{Confirm, Select, Text};
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{InputHandle, filter_video_inputs},
    outputs::{AudioEncoder, VideoEncoder, VideoResolution, scene::Scene},
};

const MOQ_CLIENT_DEFAULT_URL: &str = "https://localhost:443";
const MOQ_CLIENT_DEFAULT_BROADCAST_PATH: &str = "anon/test";

#[derive(Debug, Display, EnumIter, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoqContainer {
    #[strum(to_string = "cmaf")]
    Cmaf,

    #[strum(to_string = "legacy")]
    Legacy,

    #[strum(to_string = "loc")]
    Loc,
}

#[derive(Debug, Display, Clone)]
pub enum MoqClientRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoqClientOutput {
    pub name: String,
    endpoint_url: String,
    broadcast_path: String,
    container: MoqContainer,
    video: Option<MoqClientOutputVideoOptions>,
    audio: Option<MoqClientOutputAudioOptions>,
}

impl MoqClientOutput {
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        json!({
            "type": "moq_client",
            "endpoint_url": self.endpoint_url,
            "broadcast_path": self.broadcast_path,
            "container": self.container.to_string(),
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
        json!({
            "video": self.video.as_ref().map(|v| v.serialize_update(inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }

    pub fn on_before_registration(&mut self) -> Result<()> {
        println!(
            "Make sure that a MoQ relay is listening at {}.",
            self.endpoint_url
        );

        loop {
            let confirmation = Confirm::new("Is server running? [Y/n]")
                .with_default(true)
                .prompt()?;
            if confirmation {
                return Ok(());
            }
        }
    }
}

pub struct MoqClientOutputBuilder {
    name: String,
    endpoint_url: String,
    broadcast_path: String,
    container: MoqContainer,
    video: Option<MoqClientOutputVideoOptions>,
    audio: Option<MoqClientOutputAudioOptions>,
}

impl MoqClientOutputBuilder {
    pub fn new() -> Self {
        let name = Self::generate_name();
        let endpoint_url = MOQ_CLIENT_DEFAULT_URL.to_string();
        let broadcast_path = MOQ_CLIENT_DEFAULT_BROADCAST_PATH.to_string();
        Self {
            name,
            endpoint_url,
            broadcast_path,
            container: MoqContainer::Cmaf,
            video: None,
            audio: None,
        }
    }

    fn generate_name() -> String {
        static LAST_OUTPUT: OnceLock<AtomicU32> = OnceLock::new();
        let atomic_suffix = LAST_OUTPUT.get_or_init(|| AtomicU32::new(0));
        let suffix = atomic_suffix.fetch_add(1, Ordering::Relaxed);
        format!("output_moq_client_{suffix}")
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self
            .prompt_url()?
            .prompt_broadcast_path()?
            .prompt_container()?;

        loop {
            builder = builder.prompt_video()?.prompt_audio()?;

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either audio or video has to be specified.");
            } else {
                return Ok(builder);
            }
        }
    }

    fn prompt_url(self) -> Result<Self> {
        let url_input = Text::new("MoQ relay URL (ESC for default):")
            .with_initial_value(&self.endpoint_url)
            .prompt_skippable()?;

        match url_input {
            Some(url) if !url.trim().is_empty() => Ok(self.with_endpoint_url(url)),
            None | Some(_) => Ok(self),
        }
    }

    fn prompt_broadcast_path(self) -> Result<Self> {
        let broadcast_path_input = Text::new("Broadcast path (ESC for default):")
            .with_initial_value(&self.broadcast_path)
            .prompt_skippable()?;

        match broadcast_path_input {
            Some(broadcast_path) if !broadcast_path.trim().is_empty() => {
                Ok(self.with_broadcast_path(broadcast_path))
            }
            None | Some(_) => Ok(self),
        }
    }

    fn prompt_container(self) -> Result<Self> {
        let container_options = MoqContainer::iter().collect();
        let container_choice = Select::new("Select container (ESC for cmaf):", container_options)
            .prompt_skippable()?;
        match container_choice {
            Some(container) => Ok(self.with_container(container)),
            None => Ok(self),
        }
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![
            MoqClientRegisterOptions::SetVideoStream,
            MoqClientRegisterOptions::Skip,
        ];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(MoqClientRegisterOptions::SetVideoStream) => {
                let mut video = MoqClientOutputVideoOptions::default();
                let scene_options = Scene::iter().collect();
                let scene_choice =
                    Select::new("Select scene:", scene_options).prompt_skippable()?;
                if let Some(scene) = scene_choice {
                    video.scene = scene;
                }

                let encoder_options = vec![
                    VideoEncoder::FfmpegH264,
                    VideoEncoder::FfmpegH264LowLatency,
                    VideoEncoder::VulkanH264,
                    VideoEncoder::FfmpegVp8,
                    VideoEncoder::FfmpegVp9,
                ];

                let encoder_choice =
                    Select::new("Select encoder (ESC for ffmpeg_h264)", encoder_options)
                        .prompt_skippable()?;
                if let Some(encoder) = encoder_choice {
                    video.encoder = encoder;
                }
                Ok(self.with_video(video))
            }
            Some(MoqClientRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![
            MoqClientRegisterOptions::SetAudioStream,
            MoqClientRegisterOptions::Skip,
        ];

        let audio_selection = Select::new("Set audio stream?", audio_options).prompt_skippable()?;

        match audio_selection {
            Some(MoqClientRegisterOptions::SetAudioStream) => {
                let mut audio = MoqClientOutputAudioOptions::default();
                let encoder_options = vec![AudioEncoder::Opus, AudioEncoder::Aac];
                let encoder_choice = Select::new("Select encoder (ESC for opus)", encoder_options)
                    .prompt_skippable()?;
                if let Some(encoder) = encoder_choice {
                    audio.encoder = encoder;
                }
                Ok(self.with_audio(audio))
            }
            Some(MoqClientRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    pub fn with_endpoint_url(mut self, url: String) -> Self {
        self.endpoint_url = url;
        self
    }

    pub fn with_broadcast_path(mut self, broadcast_path: String) -> Self {
        self.broadcast_path = broadcast_path;
        self
    }

    pub fn with_container(mut self, container: MoqContainer) -> Self {
        self.container = container;
        self
    }

    pub fn with_video(mut self, video: MoqClientOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: MoqClientOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn build(self) -> MoqClientOutput {
        MoqClientOutput {
            name: self.name,
            endpoint_url: self.endpoint_url,
            broadcast_path: self.broadcast_path,
            container: self.container,
            video: self.video,
            audio: self.audio,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoqClientOutputVideoOptions {
    root_id: String,
    resolution: VideoResolution,
    encoder: VideoEncoder,
    scene: Scene,
}

impl MoqClientOutputVideoOptions {
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

impl Default for MoqClientOutputVideoOptions {
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
pub struct MoqClientOutputAudioOptions {
    encoder: AudioEncoder,
}

impl MoqClientOutputAudioOptions {
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
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

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
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

impl Default for MoqClientOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Opus,
        }
    }
}
