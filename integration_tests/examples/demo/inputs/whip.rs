use std::env;

use anyhow::Result;
use inquire::{Confirm, Text};
use rand::RngCore;
use serde_json::json;
use tracing::{error, info};

use crate::{
    inputs::{InputHandler, VideoDecoder},
    players::InputPlayer,
};

const WHIP_TOKEN_ENV: &str = "WHIP_INPUT_BEARER_TOKEN";
const WHIP_URL_ENV: &str = "WHIP_INPUT_URL";

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
    endpoint_url: Option<String>,
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
            endpoint_url: None,
            bearer_token: "example".to_string(),
            video: None,
            player: InputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        loop {
            let endpoint_url_input =
                Text::new("Enter the WHIP endpoint URL (ESC to try env WHIP_INPUT_URL):")
                    .prompt_skippable()?;

            match endpoint_url_input {
                Some(url) if !url.trim().is_empty() => {
                    builder = builder.with_endpoint_url(url);
                    break;
                }
                None | Some(_) => match env::var(WHIP_URL_ENV).ok() {
                    Some(url) => {
                        info!("WHIP endpoint url read from env: {url}");
                        builder = builder.with_endpoint_url(url);
                        break;
                    }
                    None => {
                        error!("Environment variable {WHIP_URL_ENV} not found or invalid. Please enter the URL manually.");
                    }
                },
            }
        }

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

    pub fn with_endpoint_url(mut self, url: String) -> Self {
        self.endpoint_url = Some(url);
        self
    }

    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.bearer_token = token;
        self
    }

    fn serialize(&self) -> serde_json::Value {
        let endpoint_url = self.endpoint_url.as_ref().unwrap();
        json!({
            "type": "whip_server",
            "endpoint_url": endpoint_url,
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
