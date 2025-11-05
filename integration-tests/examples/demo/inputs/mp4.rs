use std::{env, path::PathBuf, str::FromStr};

use anyhow::Result;
use inquire::{Confirm, Select, Text};
use integration_tests::{
    assets::{BUNNY_H264_PATH, BUNNY_H264_URL},
    examples::{AssetData, download_asset},
    paths::integration_tests_root,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info};

use crate::{autocompletion::FilePathCompleter, inputs::VideoDecoder, utils::resolve_path};

const MP4_INPUT_SOURCE: &str = "MP4_INPUT_SOURCE";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "Mp4InputOptions", into = "Mp4InputOptions")]
pub struct Mp4Input {
    pub name: String,
    options: Mp4InputOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mp4InputOptions {
    source: Mp4InputSource,
    decoder: VideoDecoder,

    #[serde(rename = "loop")]
    input_loop: bool,
}

impl From<Mp4InputOptions> for Mp4Input {
    fn from(value: Mp4InputOptions) -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("mp4_input_{suffix}");
        Self {
            name,
            options: value,
        }
    }
}

impl From<Mp4Input> for Mp4InputOptions {
    fn from(value: Mp4Input) -> Self {
        value.options
    }
}

impl Mp4Input {
    pub fn serialize_register(&self) -> serde_json::Value {
        let Mp4InputOptions {
            ref source,
            input_loop,
            decoder,
        } = self.options;
        let (source_key, source_val) = source.serialize();
        json!({
            "type": "mp4",
            source_key: source_val,
            "loop": input_loop,
            "decoder_map": {
                "h264": decoder.to_string(),
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mp4InputSource {
    Path(PathBuf),
    Url(String),
}

impl Mp4InputSource {
    fn serialize(&self) -> (String, String) {
        match self {
            Self::Url(url) => ("url".to_string(), url.to_string()),
            Self::Path(path) => ("path".to_string(), path.to_str().unwrap().to_string()),
        }
    }
}

impl FromStr for Mp4InputSource {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();
        if s.starts_with("http") {
            Ok(Self::Url(s.to_string()))
        } else {
            let path = resolve_path(s.into())?;
            Ok(Self::Path(path))
        }
    }
}

pub struct Mp4InputBuilder {
    name: String,
    source: Option<Mp4InputSource>,
    decoder: Option<VideoDecoder>,
    input_loop: bool,
}

impl Mp4InputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("mp4_input_{suffix}");
        Self {
            name,
            source: None,
            decoder: None,
            input_loop: false,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_source()?.prompt_decoder()?.prompt_loop()
    }

    fn prompt_source(self) -> Result<Self> {
        let env_source = env::var(MP4_INPUT_SOURCE).unwrap_or_default();
        let default_path = integration_tests_root().join(BUNNY_H264_PATH);

        loop {
            let source_input = Text::new(&format!(
                "Input path or url (ESC for {}):",
                default_path.to_str().unwrap(),
            ))
            .with_autocomplete(FilePathCompleter::default())
            .with_initial_value(&env_source)
            .prompt_skippable()?;

            match source_input {
                Some(source_str) if !source_str.trim().is_empty() => {
                    let source = Mp4InputSource::from_str(&source_str)?;
                    match &source {
                        Mp4InputSource::Path(path) if !path.exists() => {
                            error!("Path is not valid")
                        }
                        Mp4InputSource::Url(_) | Mp4InputSource::Path(_) => {
                            break Ok(self.with_source(source));
                        }
                    };
                }
                Some(_) | None => {
                    info!(
                        "Using default asset at \"{}\"",
                        default_path.to_str().unwrap()
                    );
                    download_asset(&AssetData {
                        url: BUNNY_H264_URL.to_string(),
                        path: default_path.clone(),
                    })?;
                    let source = Mp4InputSource::Path(default_path);
                    break Ok(self.with_source(source));
                }
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

    fn prompt_loop(self) -> Result<Self> {
        let loop_selection = Confirm::new("Loop input [y/N]:")
            .with_default(false)
            .prompt_skippable()?;
        match loop_selection {
            Some(r#loop) => Ok(self.with_loop(r#loop)),
            None => Ok(self),
        }
    }

    pub fn with_source(mut self, source: Mp4InputSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_decoder(mut self, decoder: VideoDecoder) -> Self {
        self.decoder = Some(decoder);
        self
    }

    pub fn with_loop(mut self, r#loop: bool) -> Self {
        self.input_loop = r#loop;
        self
    }

    pub fn build(self) -> Mp4Input {
        let options = Mp4InputOptions {
            source: self.source.unwrap(),
            decoder: self.decoder.unwrap(),
            input_loop: self.input_loop,
        };
        Mp4Input {
            name: self.name,
            options,
        }
    }
}
