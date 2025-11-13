use std::env;

use anyhow::Result;
use inquire::{Confirm, Select, Text};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};

use crate::inputs::VideoDecoder;

const WHEP_TOKEN_ENV: &str = "WHEP_INPUT_BEARER_TOKEN";
const WHEP_URL_ENV: &str = "WHEP_INPUT_URL";

#[derive(Debug, Display, EnumIter, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhepInputPlayer {
    #[strum(to_string = "Fishjam")]
    Fishjam,

    #[strum(to_string = "Manual")]
    Manual,
}

#[derive(Debug, Display, EnumIter, Clone)]
pub enum WhepRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "WhepInputOptions", into = "WhepInputOptions")]
pub struct WhepInput {
    pub name: String,
    options: WhepInputOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhepInputOptions {
    endpoint_url: String,
    bearer_token: String,
    video: Option<WhepInputVideoOptions>,
    player: WhepInputPlayer,
}

impl From<WhepInputOptions> for WhepInput {
    fn from(value: WhepInputOptions) -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("input_whep_{suffix}");
        Self {
            name,
            options: value,
        }
    }
}

impl From<WhepInput> for WhepInputOptions {
    fn from(value: WhepInput) -> Self {
        value.options
    }
}

impl WhepInput {
    pub fn has_video(&self) -> bool {
        self.options.video.is_some()
    }

    pub fn serialize_register(&self) -> serde_json::Value {
        let WhepInputOptions {
            endpoint_url,
            bearer_token,
            video,
            ..
        } = &self.options;
        json!({
            "type": "whep_client",
            "endpoint_url": endpoint_url,
            "bearer_token": bearer_token,
            "video": video.as_ref().map(|v| v.serialize_register()),
        })
    }

    pub fn on_before_registration(&mut self) -> Result<()> {
        match self.options.player {
            WhepInputPlayer::Manual => {
                let cmd = "docker run -e UDP_MUX_PORT=8080 -e NAT_1_TO_1_IP=127.0.0.1 -e NETWORK_TEST_ON_START=false -p 8080:8080 -p 8080:8080/udp seaduboi/broadcast-box";
                let url = "http://127.0.0.1:8080";

                println!("Instructions to start streaming:");
                println!("1. Start Broadcast Box: {cmd}");
                println!("2. Open: {url}");
                println!("3. Make sure that 'I want to stream' option is selected.");
                println!(
                    "4. Enter '{}' in 'Stream Key' field",
                    self.options.bearer_token,
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
            WhepInputPlayer::Fishjam => Ok(()),
        }
    }
}

pub struct WhepInputBuilder {
    name: String,
    endpoint_url: String,
    bearer_token: String,
    video: Option<WhepInputVideoOptions>,
    player: WhepInputPlayer,
}

impl WhepInputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("input_whep_{suffix}");

        // Broadcast Box output url
        let endpoint_url = "http://127.0.0.1:8080/api/whep".to_string();
        let bearer_token = "example".to_string();
        Self {
            name,
            endpoint_url,
            bearer_token,
            video: None,
            player: WhepInputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_video()?
            .prompt_player()?
            .prompt_url()?
            .prompt_bearer_token()
    }

    fn prompt_player(self) -> Result<Self> {
        let player_options = WhepInputPlayer::iter().collect();
        let player_selection =
            Select::new("Select input player (ESC for Manual): ", player_options)
                .prompt_skippable()?;
        match player_selection {
            Some(player) => Ok(self.with_player(player)),
            None => Ok(self),
        }
    }

    fn prompt_url(self) -> Result<Self> {
        match self.player {
            WhepInputPlayer::Manual => {
                let env_url = env::var(WHEP_URL_ENV).unwrap_or_default();
                let endpoint_url_input =
                    Text::new("Enter the WHEP endpoint URL (ESC for BroadcastBox):")
                        .with_initial_value(&env_url)
                        .prompt_skippable()?;

                match endpoint_url_input {
                    Some(url) if !url.trim().is_empty() => Ok(self.with_endpoint_url(url)),
                    Some(_) | None => Ok(self),
                }
            }
            WhepInputPlayer::Fishjam => {
                const FISHJAM_URL: &str = "https://fishjam.io/api/v1/live/api/whep";
                Ok(self.with_endpoint_url(FISHJAM_URL.to_string()))
            }
        }
    }

    fn prompt_bearer_token(self) -> Result<Self> {
        let env_token = env::var(WHEP_TOKEN_ENV).unwrap_or_default();
        if self.player == WhepInputPlayer::Fishjam {
            println!();
            println!(
                "1. Visit https://fishjam.io and sign in. Create an account if you don't have one."
            );
            println!("2. Copy your Fishjam ID from dashboard.");
            println!("3. Visit https://livestreaming.fishjam.io/");
            println!("4. Open network tab in dev tools and reload if necessary.");
            println!("5. Paste copied Fishjam ID in the appropriate input field.");
            println!("6. Start streaming and press \"Connect to stream\".");
            println!("7. In dev tools search for the request named \"whep\" using POST method.");
            println!(
                "8. In \"Request headers\" section find \"Authorization\" header and copy a token from there."
            );
            println!("9. Disconnect from watching the stream before pasting the token.");
        }
        let token_input = Text::new("Enter the WHEP bearer token. (ESC for \"example\"):")
            .with_initial_value(&env_token)
            .prompt_skippable()?;
        match token_input {
            Some(token) if !token.trim().is_empty() => Ok(self.with_bearer_token(token)),
            Some(_) | None => Ok(self.with_bearer_token("example".to_string())),
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
        self.endpoint_url = url;
        self
    }

    pub fn with_bearer_token(mut self, token: String) -> Self {
        self.bearer_token = token;
        self
    }

    pub fn with_player(mut self, player: WhepInputPlayer) -> Self {
        self.player = player;
        self
    }

    pub fn build(self) -> WhepInput {
        let options = WhepInputOptions {
            endpoint_url: self.endpoint_url,
            bearer_token: self.bearer_token,
            video: self.video,
            player: self.player,
        };
        WhepInput {
            name: self.name,
            options,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
