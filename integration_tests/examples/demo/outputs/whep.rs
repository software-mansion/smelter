use std::env;

use anyhow::Result;
use inquire::{Confirm, Select, Text};
use rand::RngCore;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{filter_video_inputs, InputHandler},
    outputs::{scene::Scene, AudioEncoder, OutputHandler, VideoEncoder, VideoResolution},
    players::OutputPlayer,
};

const WHEP_TOKEN_ENV: &str = "WHEP_OUTPUT_BEARER_TOKEN";

#[derive(Debug, Display, EnumIter, Clone)]
pub enum WhepRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug)]
pub struct WhepOutput {
    name: String,
    bearer_token: String,
    video: Option<WhepOutputVideoOptions>,
    audio: Option<WhepOutputAudioOptions>,
}

impl OutputHandler for WhepOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_after_registration(&mut self, player: OutputPlayer) -> Result<()> {
        match player {
            OutputPlayer::Manual => {
                let cmd = "docker run -e UDP_MUX_PORT=8080 -e NAT_1_TO_1_IP=127.0.0.1 -e NETWORK_TEST_ON_START=false -p 8080:8080 -p 8080:8080/udp seaduboi/broadcast-box";
                let url = "http://127.0.0.1:8080";

                println!("Instructions to start receiving stream:");
                println!("1. Start Broadcast Box: {cmd}");
                println!("2. Open: {url}");
                println!("3. Enter '{}' in 'Stream Key' field", self.bearer_token);

                loop {
                    let confirmation = Confirm::new("Is player running? [y/n]").prompt()?;
                    if confirmation {
                        return Ok(());
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    fn serialize_update(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
        json!({
           "video": self.video.as_ref().map(|v| v.serialize_update(inputs, &self.name)),
           "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }
}

pub struct WhepOutputBuilder {
    name: String,
    bearer_token: Option<String>,
    video: Option<WhepOutputVideoOptions>,
    audio: Option<WhepOutputAudioOptions>,
    player: OutputPlayer,
}

impl WhepOutputBuilder {
    pub fn new() -> Self {
        let suffix = rand::thread_rng().next_u32();
        let name = format!("output_whep_{suffix}");
        Self {
            name,
            bearer_token: None,
            video: None,
            audio: None,
            player: OutputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        builder = builder.prompt_token()?;

        let audio_options = vec![
            WhepRegisterOptions::SetAudioStream,
            WhepRegisterOptions::Skip,
        ];

        loop {
            builder = builder.prompt_video()?;

            let audio_selection =
                Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(WhepRegisterOptions::SetAudioStream) => {
                    builder.with_audio(WhepOutputAudioOptions::default())
                }
                Some(WhepRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either audio or video has to be specified for WHEP output");
            } else {
                break;
            }
        }

        Ok(builder)
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![
            WhepRegisterOptions::SetVideoStream,
            WhepRegisterOptions::Skip,
        ];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(WhepRegisterOptions::SetVideoStream) => {
                let scene_options = Scene::iter().collect();
                let scene_choice =
                    Select::new("Select scene:", scene_options).prompt_skippable()?;
                let video = match scene_choice {
                    Some(scene) => WhepOutputVideoOptions {
                        scene,
                        ..Default::default()
                    },
                    None => WhepOutputVideoOptions::default(),
                };
                Ok(self.with_video(video))
            }
            Some(WhepRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_token(self) -> Result<Self> {
        let env_token = env::var(WHEP_TOKEN_ENV).unwrap_or_default();
        loop {
            let endpoint_token_input = Text::new("Enter the WHEP endpoint bearer token:")
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

    pub fn with_video(mut self, video: WhepOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: WhepOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.bearer_token = Some(token);
        self
    }

    fn serialize(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
        let bearer_token = self.bearer_token.as_ref().unwrap();
        json!({
            "type": "whep_server",
            "bearer_token": bearer_token,
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs, &self.name)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    pub fn build(
        self,
        inputs: &[&dyn InputHandler],
    ) -> (WhepOutput, serde_json::Value, OutputPlayer) {
        let register_request = self.serialize(inputs);

        let whep_output = WhepOutput {
            name: self.name,
            bearer_token: self.bearer_token.unwrap(),
            video: self.video,
            audio: self.audio,
        };

        (whep_output, register_request, self.player)
    }
}

#[derive(Debug)]
pub struct WhepOutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
    root_id: String,
    scene: Scene,
}

impl WhepOutputVideoOptions {
    pub fn serialize_register(
        &self,
        inputs: &[&dyn InputHandler],
        output_name: &str,
    ) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "resolution": self.resolution.serialize(),
            "encoder": {
                "type": self.encoder.to_string(),
            },
            "initial": {
                "root": self.scene.serialize(&self.root_id, &inputs, output_name, self.resolution),
            },
        })
    }

    pub fn serialize_update(
        &self,
        inputs: &[&dyn InputHandler],
        output_name: &str,
    ) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "root": self.scene.serialize(&self.root_id, &inputs, output_name, self.resolution),
        })
    }
}

impl Default for WhepOutputVideoOptions {
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
            scene: Scene::Tiles,
        }
    }
}

#[derive(Debug)]
pub struct WhepOutputAudioOptions {
    encoder: AudioEncoder,
}

impl WhepOutputAudioOptions {
    pub fn serialize_register(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
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

    pub fn serialize_update(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
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

impl Default for WhepOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Opus,
        }
    }
}
