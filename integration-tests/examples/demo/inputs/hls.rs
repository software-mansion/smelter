use std::{env, process::Command};

use anyhow::Result;
use inquire::{Select, Text};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::error;

use crate::inputs::VideoDecoder;

const HLS_INPUT_URL: &str = "HLS_INPUT_URL";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "HlsInputOptions", into = "HlsInputOptions")]
pub struct HlsInput {
    name: String,
    options: HlsInputOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlsInputOptions {
    url: String,
    decoder: VideoDecoder,
}

impl From<HlsInputOptions> for HlsInput {
    fn from(value: HlsInputOptions) -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("hls_input_{suffix}");
        Self {
            name,
            options: value,
        }
    }
}

impl From<HlsInput> for HlsInputOptions {
    fn from(value: HlsInput) -> Self {
        value.options
    }
}

impl HlsInput {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn serialize_register(&self) -> serde_json::Value {
        let HlsInputOptions { ref url, decoder } = self.options;
        json!({
            "type": "hls",
            "url": url,
            "decoder_map": {
                "h264": decoder.to_string(),
            },
        })
    }
}

pub struct HlsInputBuilder {
    name: String,
    url: Option<String>,
    decoder: Option<VideoDecoder>,
}

impl HlsInputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("hls_input_{suffix}");
        Self {
            name,
            url: None,
            decoder: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_url()?.prompt_decoder()
    }

    fn prompt_url(self) -> Result<Self> {
        const DEFAULT_URL: &str = "https://raw.githubusercontent.com/membraneframework/membrane_http_adaptive_stream_plugin/master/test/membrane_http_adaptive_stream/integration_test/fixtures/audio_multiple_video_tracks/index.m3u8";
        let env_url = env::var(HLS_INPUT_URL).unwrap_or_default();
        loop {
            let hls_url = Text::new("HLS input url (ESC for default):")
                .with_initial_value(&env_url)
                .prompt_skippable()?;

            match hls_url {
                Some(url) if !url.trim().is_empty() => {
                    const STREAMLINK_PREFIXES: [&str; 3] = [
                        "https://www.twitch.tv/",
                        "https://www.youtube.com/",
                        "https://kick.com/",
                    ];

                    let url = if STREAMLINK_PREFIXES
                        .iter()
                        .any(|prefix| url.starts_with(prefix))
                    {
                        let streamlink_output = Command::new("streamlink")
                            .arg("--stream-url")
                            .arg(&url)
                            .output();
                        match streamlink_output {
                            Ok(output) if output.status.code() == Some(0) => {
                                String::from_utf8(output.stdout)?
                            }
                            Ok(output) => {
                                error!("`streamlink` failed with code {:?}.", output.status.code());
                                continue;
                            }
                            Err(error) => {
                                error!(%error, "`streamlink` failed.");
                                continue;
                            }
                        }
                    } else {
                        url
                    };
                    return Ok(self.with_url(url.trim().to_string()));
                }
                Some(_) | None => return Ok(self.with_url(DEFAULT_URL.to_string())),
            }
        }
    }

    fn prompt_decoder(self) -> Result<Self> {
        let decoder_options = vec![VideoDecoder::FfmpegH264, VideoDecoder::VulkanH264];
        let decoder_selection =
            Select::new("Select decoder: (ESC for ffmpeg_h264)", decoder_options)
                .prompt_skippable()?;

        match decoder_selection {
            Some(decoder) => Ok(self.with_decoder(decoder)),
            None => Ok(self.with_decoder(VideoDecoder::FfmpegH264)),
        }
    }

    pub fn with_url(mut self, url: String) -> Self {
        self.url = Some(url);
        self
    }

    pub fn with_decoder(mut self, decoder: VideoDecoder) -> Self {
        self.decoder = Some(decoder);
        self
    }

    pub fn build(self) -> HlsInput {
        let options = HlsInputOptions {
            url: self.url.unwrap(),
            decoder: self.decoder.unwrap(),
        };
        HlsInput {
            name: self.name,
            options,
        }
    }
}
