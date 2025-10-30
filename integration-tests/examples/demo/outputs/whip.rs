use std::env;

use anyhow::Result;
use inquire::{Confirm, Select, Text};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{InputHandle, filter_video_inputs},
    outputs::{AudioEncoder, OutputHandle, VideoEncoder, VideoResolution, scene::Scene},
};

const WHIP_TOKEN_ENV: &str = "WHIP_OUTPUT_BEARER_TOKEN";
const WHIP_URL_ENV: &str = "WHIP_OUTPUT_URL";

#[derive(Debug, Display, EnumIter, Clone)]
pub enum WhipRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WhipOutput {
    name: String,
    endpoint_url: String,
    bearer_token: String,
    video: Option<WhipOutputVideoOptions>,
    audio: Option<WhipOutputAudioOptions>,
}

#[typetag::serde]
impl OutputHandle for WhipOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        json!({
            "type": "whip_client",
            "endpoint_url": self.endpoint_url,
            "bearer_token": self.bearer_token,
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    fn on_before_registration(&mut self) -> Result<()> {
        let cmd = "docker run -e UDP_MUX_PORT=8080 -e NAT_1_TO_1_IP=127.0.0.1 -e NETWORK_TEST_ON_START=false -p 8080:8080 -p 8080:8080/udp seaduboi/broadcast-box";
        let url = "http://127.0.0.1:8080";

        println!("Instructions to start receiving stream:");
        println!("1. Start Broadcast Box: {cmd}");
        println!("2. Open: {url}");
        println!("3. Make sure that 'I want to watch' option is selected.");
        println!("4. Enter '{}' in 'Stream Key' field", self.bearer_token);

        loop {
            let confirmation = Confirm::new("Is player running? [Y/n]")
                .with_default(true)
                .prompt()?;
            if confirmation {
                return Ok(());
            }
        }
    }

    fn serialize_update(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        json!({
           "video": self.video.as_ref().map(|v| v.serialize_update(inputs)),
           "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }
}

pub struct WhipOutputBuilder {
    name: String,
    endpoint_url: Option<String>,
    bearer_token: Option<String>,
    video: Option<WhipOutputVideoOptions>,
    audio: Option<WhipOutputAudioOptions>,
}

impl WhipOutputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("output_whip_{suffix}");
        Self {
            name,
            endpoint_url: None,
            bearer_token: None,
            video: None,
            audio: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_url()?
            .prompt_token()?
            .prompt_video()?
            .prompt_audio()
    }

    fn prompt_url(self) -> Result<Self> {
        const BROADCAST_BOX_URL: &str = "http://127.0.0.1:8080/api/whip";
        let env_url = env::var(WHIP_URL_ENV).unwrap_or_default();
        let endpoint_url_input = Text::new("Enter the WHIP endpoint URL (ESC for BroadcastBox):")
            .with_initial_value(&env_url)
            .prompt_skippable()?;

        match endpoint_url_input {
            Some(url) if !url.trim().is_empty() => Ok(self.with_endpoint_url(url)),
            Some(_) | None => Ok(self.with_endpoint_url(BROADCAST_BOX_URL.to_string())),
        }
    }

    fn prompt_token(self) -> Result<Self> {
        let env_token = env::var(WHIP_TOKEN_ENV).unwrap_or_default();
        loop {
            let endpoint_token_input = Text::new("Enter the WHIP endpoint bearer token:")
                .with_initial_value(&env_token)
                .prompt_skippable()?;

            match endpoint_token_input {
                Some(token) if !token.trim().is_empty() => return Ok(self.with_bearer_token(token)),
                Some(_) | None => {
                    error!("Bearer token cannot be empty.");
                    continue;
                }
            }
        }
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![
            WhipRegisterOptions::SetVideoStream,
            WhipRegisterOptions::Skip,
        ];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(WhipRegisterOptions::SetVideoStream) => {
                let mut video = WhipOutputVideoOptions::default();
                let mut encoder_options = VideoEncoder::iter().collect::<Vec<_>>();
                let mut encoder_preferences = vec![];
                loop {
                    let encoder_selection = Select::new(
                        "Select encoder (ESC or Any to progress):",
                        encoder_options.clone(),
                    )
                    .prompt_skippable()?;

                    match encoder_selection {
                        Some(encoder) => {
                            encoder_preferences.push(encoder);
                            if encoder == VideoEncoder::Any {
                                break;
                            } else {
                                encoder_options.retain(|enc| *enc != encoder);
                            }
                        }
                        None => break,
                    }
                }
                video.encoder_preferences = encoder_preferences;

                let scene_options = Scene::iter().collect();
                let scene_choice =
                    Select::new("Select scene:", scene_options).prompt_skippable()?;
                if let Some(scene) = scene_choice {
                    video.scene = scene;
                }
                Ok(self.with_video(video))
            }
            Some(WhipRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let mut audio = WhipOutputAudioOptions::default();

        let mut encoder_options = vec![AudioEncoder::Any, AudioEncoder::Opus];
        let mut encoder_preferences = vec![];
        loop {
            let encoder_selection = Select::new(
                "Select encoder (ESC or Any to progress):",
                encoder_options.clone(),
            )
            .prompt_skippable()?;

            match encoder_selection {
                Some(encoder) => {
                    encoder_preferences.push(encoder);
                    if encoder == AudioEncoder::Any {
                        break;
                    } else {
                        encoder_options.retain(|enc| *enc != encoder);
                    }
                }
                None => break,
            }
        }
        audio.encoder_preferences = encoder_preferences;

        Ok(self.with_audio(audio))
    }

    pub fn with_video(mut self, video: WhipOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: WhipOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_endpoint_url(mut self, url: String) -> Self {
        self.endpoint_url = Some(url);
        self
    }

    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.bearer_token = Some(token);
        self
    }

    pub fn build(self) -> WhipOutput {
        WhipOutput {
            name: self.name,
            endpoint_url: self.endpoint_url.unwrap(),
            bearer_token: self.bearer_token.unwrap(),
            video: self.video,
            audio: self.audio,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WhipOutputVideoOptions {
    resolution: VideoResolution,
    encoder_preferences: Vec<VideoEncoder>,
    root_id: String,
    scene: Scene,
}

impl WhipOutputVideoOptions {
    fn serialize_encoder_preferences(&self) -> Vec<serde_json::Value> {
        self.encoder_preferences
            .iter()
            .map(|enc| {
                json!({
                    "type": enc.to_string(),
                })
            })
            .collect()
    }

    pub fn serialize_register(&self, inputs: &[&dyn InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "resolution": self.resolution.serialize(),
            "encoder_preferences": self.serialize_encoder_preferences(),
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

impl Default for WhipOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        let root_id = "root".to_string();
        Self {
            resolution,
            encoder_preferences: vec![],
            root_id,
            scene: Scene::Tiles,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct WhipOutputAudioOptions {
    encoder_preferences: Vec<AudioEncoder>,
}

impl WhipOutputAudioOptions {
    fn serialize_encoder_preferences(&self) -> Vec<serde_json::Value> {
        self.encoder_preferences
            .iter()
            .map(|enc| {
                json!({
                    "type": enc.to_string(),
                })
            })
            .collect()
    }

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
            "encoder_preferences": self.serialize_encoder_preferences(),
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
