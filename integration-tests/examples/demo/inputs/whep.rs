use std::env;

use anyhow::Result;
use inquire::{Confirm, Select, Text};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::info;

use crate::inputs::{InputHandle, VideoDecoder};

const WHEP_TOKEN_ENV: &str = "WHEP_INPUT_BEARER_TOKEN";
const WHEP_URL_ENV: &str = "WHEP_INPUT_URL";

#[derive(Debug, Display, EnumIter, Clone)]
pub enum WhepRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WhepInput {
    name: String,
    endpoint_url: String,
    bearer_token: String,
    video: Option<WhepInputVideoOptions>,
}

#[typetag::serde]
impl InputHandle for WhepInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self) -> serde_json::Value {
        json!({
            "type": "whep_client",
            "endpoint_url": self.endpoint_url,
            "bearer_token": self.bearer_token,
            "video": self.video.as_ref().map(|v| v.serialize_register()),
        })
    }

    fn has_video(&self) -> bool {
        self.video.is_some()
    }

    fn on_before_registration(&mut self) -> Result<()> {
        let cmd = "docker run -e UDP_MUX_PORT=8080 -e NAT_1_TO_1_IP=127.0.0.1 -e NETWORK_TEST_ON_START=false -p 8080:8080 -p 8080:8080/udp seaduboi/broadcast-box";
        let url = "http://127.0.0.1:8080";

        println!("Instructions to start streaming:");
        println!("1. Start Broadcast Box: {cmd}");
        println!("2. Open: {url}");
        println!("3. Make sure that 'I want to stream' option is selected.");
        println!("4. Enter '{}' in 'Stream Key' field", self.bearer_token);

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

pub struct WhepInputBuilder {
    name: String,
    endpoint_url: Option<String>,
    bearer_token: String,
    video: Option<WhepInputVideoOptions>,
}

impl WhepInputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("input_whep_{suffix}");
        Self {
            name,
            endpoint_url: None,
            bearer_token: "example".to_string(),
            video: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_url()?.prompt_bearer_token()?.prompt_video()
    }

    pub fn prompt_url(self) -> Result<Self> {
        const BROADCAST_BOX_URL: &str = "http://127.0.0.1:8080/api/whep";
        let env_url = env::var(WHEP_URL_ENV).unwrap_or_default();
        let endpoint_url_input = Text::new("Enter the WHEP endpoint URL (ESC for BroadcastBox):")
            .with_initial_value(&env_url)
            .prompt_skippable()?;

        match endpoint_url_input {
            Some(url) if !url.trim().is_empty() => Ok(self.with_endpoint_url(url)),
            Some(_) | None => Ok(self.with_endpoint_url(BROADCAST_BOX_URL.to_string())),
        }
    }

    // It doesn't actually prompt, but is used in chain
    fn prompt_bearer_token(self) -> Result<Self> {
        match env::var(WHEP_TOKEN_ENV).ok() {
            Some(token) => {
                info!("WHEP bearer token read from env: {token}");
                Ok(self.with_bearer_token(token))
            }
            None => {
                info!("Using default WHEP bearer token '{}'", self.bearer_token);
                Ok(self)
            }
        }
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = WhepRegisterOptions::iter().collect();
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(WhepRegisterOptions::SetVideoStream) => {
                let mut video = WhepInputVideoOptions::default();
                let mut decoder_options = VideoDecoder::iter().collect::<Vec<_>>();
                let mut decoder_preferences = vec![];
                loop {
                    let decoder_selection = Select::new(
                        "Select decoder (ESC or Any to progress):",
                        decoder_options.clone(),
                    )
                    .prompt_skippable()?;

                    match decoder_selection {
                        Some(decoder) => {
                            decoder_preferences.push(decoder);
                            if decoder == VideoDecoder::Any {
                                break;
                            } else {
                                decoder_options.retain(|dec| *dec != decoder);
                            }
                        }
                        None => break,
                    }
                }
                video.decoder_preferences = decoder_preferences;

                Ok(self.with_video(video))
            }
            Some(WhepRegisterOptions::Skip) | None => Ok(self),
        }
    }

    pub fn with_video(mut self, video: WhepInputVideoOptions) -> Self {
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

    pub fn build(self) -> WhepInput {
        WhepInput {
            name: self.name,
            endpoint_url: self.endpoint_url.unwrap(),
            bearer_token: self.bearer_token,
            video: self.video,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct WhepInputVideoOptions {
    decoder_preferences: Vec<VideoDecoder>,
}

impl WhepInputVideoOptions {
    pub fn serialize_register(&self) -> serde_json::Value {
        json!({
            "decoder_preferences": self.decoder_preferences.iter().map(|dec| dec.to_string()).collect::<Vec<_>>(),
        })
    }
}
