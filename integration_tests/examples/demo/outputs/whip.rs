use std::env;

use anyhow::{anyhow, Result};
use inquire::{Select, Text};
use rand::RngCore;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    outputs::{AudioEncoder, VideoEncoder, VideoResolution},
    players::OutputPlayer,
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

pub struct WhipOutput {
    name: String,
    endpoint_url: String,
    bearer_token: Option<String>,
    video: Option<WhipOutputVideoOptions>,
    audio: Option<WhipOutputAudioOptions>,
}

pub struct WhipOutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
    root_id: String,
}

impl WhipOutputVideoOptions {
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
            "encoder_preferences": self.encoder.to_string(),
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

impl Default for WhipOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        let suffix = rand::thread_rng().next_u32();
        let root_id = format!("tiles_{suffix}");
        Self {
            resolution,
            encoder: VideoEncoder::Any,
            root_id,
        }
    }
}

pub struct WhipOutputAudioOptions {
    encoder: AudioEncoder,
}

impl WhipOutputAudioOptions {
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
            "encoder_preferences": [
                {
                    "type": self.encoder.to_string(),
                }
            ],
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

impl Default for WhipOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Any,
        }
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
        let name = "output_whip".to_string();
        Self {
            name,
            endpoint_url: None,
            bearer_token: None,
            video: None,
            audio: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        loop {
            let endpoint_url_input =
                Text::new("Enter the WHIP endpoint URL (ESC to try env WHIP_OUTPUT_URL):")
                    .prompt_skippable()?;

            match endpoint_url_input {
                Some(url) if !url.trim().is_empty() => {
                    builder = builder.with_endpoint_url(url);
                    break;
                }
                None | Some(_) => match env::var(WHIP_URL_ENV).ok() {
                    Some(url) => {
                        builder = builder.with_endpoint_url(url);
                        break;
                    }
                    None => {
                        error!("Environment variable {WHIP_URL_ENV} not found or invalid. Please enter the URL manually.");
                    }
                },
            }
        }

        loop {
            let bearer_token_input =
                Text::new("Enter the bearer token (ESC to try env WHIP_OUTPUT_BEARER_TOKEN):")
                    .prompt_skippable()?;

            match bearer_token_input {
                Some(token) if !token.trim().is_empty() => {
                    builder = builder.with_bearer_token(token);
                    break;
                }
                None | Some(_) => match env::var(WHIP_TOKEN_ENV).ok() {
                    Some(token) => {
                        builder = builder.with_bearer_token(token);
                        break;
                    }
                    None => {
                        error!("Environment variable {WHIP_TOKEN_ENV} not found or invalid. Please enter the token manually.");
                    }
                },
            }
        }

        let video_options = vec![
            WhipRegisterOptions::SetVideoStream,
            WhipRegisterOptions::Skip,
        ];
        let audio_options = vec![
            WhipRegisterOptions::SetAudioStream,
            WhipRegisterOptions::Skip,
        ];

        loop {
            let video_selection =
                Select::new("Set video stream?", video_options.clone()).prompt_skippable()?;

            builder = match video_selection {
                Some(WhipRegisterOptions::SetVideoStream) => {
                    builder.with_video(WhipOutputVideoOptions::default())
                }
                Some(WhipRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            let audio_selection =
                Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(WhipRegisterOptions::SetAudioStream) => {
                    builder.with_audio(WhipOutputAudioOptions::default())
                }
                Some(WhipRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            if builder.video.is_none() && builder.audio.is_none() {
                error!(
                    "At least one video or one audio stream has to be specified for WHIP output!"
                );
            } else {
                break;
            }
        }

        Ok(builder)
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

    fn serialize(&self, _inputs: &[&str]) -> serde_json::Value {
        json!({})
    }

    pub fn build(self, inputs: &[&str]) -> Result<(WhipOutput, serde_json::Value)> {
        let register_request = self.serialize(inputs);

        let endpoint_url = self
            .endpoint_url
            .ok_or_else(|| anyhow!("WHIP output requires an endpoint URL to be specified."))?;

        let whip_output = WhipOutput {
            name: self.name,
            endpoint_url,
            bearer_token: self.bearer_token,
            video: self.video,
            audio: self.audio,
        };

        Ok((whip_output, register_request))
    }
}
