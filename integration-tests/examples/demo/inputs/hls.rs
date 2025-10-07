use std::env;

use anyhow::Result;
use inquire::Text;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::inputs::InputHandle;

const HLS_INPUT_URL: &str = "HLS_INPUT_URL";

#[derive(Debug, Serialize, Deserialize)]
pub struct HlsInput {
    name: String,
    url: String,
}

#[typetag::serde]
impl InputHandle for HlsInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self) -> serde_json::Value {
        json!({
            "type": "hls",
            "url": self.url,
        })
    }
}

pub struct HlsInputBuilder {
    name: String,
    url: Option<String>,
}

impl HlsInputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("hls_input_{suffix}");
        Self { name, url: None }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_url()
    }

    fn prompt_url(self) -> Result<Self> {
        const DEFAULT_URL: &str = "https://raw.githubusercontent.com/membraneframework/membrane_http_adaptive_stream_plugin/master/test/membrane_http_adaptive_stream/integration_test/fixtures/audio_multiple_video_tracks/index.m3u8";
        let env_url = env::var(HLS_INPUT_URL).unwrap_or_default();
        let hls_url = Text::new("HLS input url (ESC for default):")
            .with_default(DEFAULT_URL)
            .with_initial_value(&env_url)
            .prompt_skippable()?;
        match hls_url {
            Some(url) => Ok(self.with_url(url)),
            None => Ok(self.with_url(DEFAULT_URL.to_string())),
        }
    }

    pub fn with_url(mut self, url: String) -> Self {
        self.url = Some(url);
        self
    }

    pub fn build(self) -> HlsInput {
        HlsInput {
            name: self.name,
            url: self.url.unwrap(),
        }
    }
}
