use std::env;

use anyhow::Result;
use inquire::Confirm;
use rand::RngCore;
use serde_json::json;
use tracing::info;

use crate::{
    inputs::{InputHandler, VideoDecoder},
    players::InputPlayer,
};

const WHIP_TOKEN_ENV: &str = "WHIP_INPUT_BEARER_TOKEN";

#[derive(Debug)]
pub struct WhipInput {
    name: String,
}

impl InputHandler for WhipInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_after_registration(&mut self, player: InputPlayer) -> Result<()> {
        match player {
            InputPlayer::Manual => loop {
                let confirmation = Confirm::new("Is player running? [y/n]").prompt()?;
                if confirmation {
                    return Ok(());
                }
            },
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
        let suffix = rand::thread_rng().next_u32();
        let name = format!("input_whip_{suffix}");
        Self {
            name,
            bearer_token: "example".to_string(),
            video: None,
            player: InputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        builder = match env::var(WHIP_TOKEN_ENV).ok() {
            Some(token) => {
                info!("WHIP bearer token read from env: {token}");
                builder.with_bearer_token(token)
            }
            None => builder,
        };

        builder = builder.with_video(WhipInputVideoOptions::default());

        Ok(builder)
    }

    pub fn with_video(mut self, video: WhipInputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.bearer_token = token;
        self
    }

    fn serialize(&self) -> serde_json::Value {
        json!({
            "type": "whip_server",
            "bearer_token": self.bearer_token,
            "video": self.video.as_ref().map(|v| v.serialize_register()),
        })
    }

    pub fn build(self) -> (WhipInput, serde_json::Value, InputPlayer) {
        let register_request = self.serialize();

        let whip_input = WhipInput { name: self.name };

        (whip_input, register_request, self.player)
    }
}

#[derive(Debug)]
pub struct WhipInputVideoOptions {
    decoder: VideoDecoder,
}

impl WhipInputVideoOptions {
    pub fn serialize_register(&self) -> serde_json::Value {
        json!({
            "decoder_preferences": [
                {
                    "type": self.decoder.to_string(),
                },
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
