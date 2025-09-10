use std::{
    env,
    sync::{atomic::AtomicU32, OnceLock},
};

use anyhow::Result;
use inquire::Text;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use crate::{
    inputs::{InputHandler, VideoDecoder},
    players::InputPlayer,
};

const WHIP_TOKEN_ENV: &str = "WHIP_INPUT_BEARER_TOKEN";

#[derive(Debug, Serialize, Deserialize)]
pub struct WhipInput {
    name: String,
    bearer_token: String,
    video: Option<WhipInputVideoOptions>,
    player: InputPlayer,
}

#[typetag::serde]
impl InputHandler for WhipInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self) -> serde_json::Value {
        json!({
            "type": "whip_server",
            "bearer_token": self.bearer_token,
            "video": self.video.as_ref().map(|v| v.serialize_register()),
        })
    }

    fn json_dump(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }

    fn has_video(&self) -> bool {
        self.video.is_some()
    }

    fn on_after_registration(&mut self) -> Result<()> {
        match self.player {
            InputPlayer::Manual => {
                println!("Instructions to start streaming:");
                println!("1. Open OBS Studio");
                println!("2. In a 'Stream' tab enter 'http://127.0.0.1:9000/whip/{} in 'Server' field and '{}' in 'Bearer Token' field", self.name, self.bearer_token);
                println!();
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}

pub struct WhipInputBuilder {
    name: String,
    bearer_token: String,
    video: Option<WhipInputVideoOptions>,
    player: InputPlayer,
}

impl WhipInputBuilder {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            bearer_token: "example".to_string(),
            video: None,
            player: InputPlayer::Manual,
        }
    }

    fn generate_name() -> String {
        static LAST_INPUT: OnceLock<AtomicU32> = OnceLock::new();
        let atomic_suffix = LAST_INPUT.get_or_init(|| AtomicU32::new(0));
        let suffix = atomic_suffix.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        format!("input_whip_{suffix}")
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        let name_input = Text::new("Input name (optional):")
            .with_initial_value(&builder.name)
            .prompt_skippable()?;

        builder = match name_input {
            Some(name) if !name.trim().is_empty() => builder.with_name(name),
            None | Some(_) => builder.with_name(WhipInputBuilder::generate_name()),
        };

        builder = match env::var(WHIP_TOKEN_ENV).ok() {
            Some(token) => {
                info!("WHIP bearer token read from env: {token}");
                builder.with_bearer_token(token)
            }
            None => {
                info!("Using default WHIP bearer token '{}'", builder.bearer_token);
                builder
            }
        };

        builder = builder.with_video(WhipInputVideoOptions::default());

        Ok(builder)
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn with_video(mut self, video: WhipInputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.bearer_token = token;
        self
    }

    pub fn build(self) -> WhipInput {
        WhipInput {
            name: self.name,
            bearer_token: self.bearer_token,
            video: self.video,
            player: self.player,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WhipInputVideoOptions {
    decoder: VideoDecoder,
}

impl WhipInputVideoOptions {
    pub fn serialize_register(&self) -> serde_json::Value {
        json!({
            "decoder_preferences": [
                self.decoder.to_string(),
            ],
        })
    }
}

impl Default for WhipInputVideoOptions {
    fn default() -> Self {
        Self {
            decoder: VideoDecoder::Any,
        }
    }
}
