use std::{env, path::PathBuf, str::FromStr};

use anyhow::Result;
use inquire::{Confirm, Text};
use integration_tests::{
    assets::{BUNNY_H264_PATH, BUNNY_H264_URL},
    examples::{download_asset, examples_root_dir, AssetData},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info};

use crate::{
    autocompletion::FilePathCompleter, inputs::InputHandler, players::InputPlayer,
    utils::resolve_path,
};

const MP4_INPUT_SOURCE: &str = "MP4_INPUT_SOURCE";

#[derive(Debug, Serialize, Deserialize)]
pub struct Mp4Input {
    name: String,
    source: Mp4InputSource,

    #[serde(rename = "loop")]
    input_loop: bool,
}

#[typetag::serde]
impl InputHandler for Mp4Input {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self) -> serde_json::Value {
        let (source_key, source_val) = self.source.serialize();
        json!({
            "type": "mp4",
            source_key: source_val,
            "loop": self.input_loop,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
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
    input_loop: bool,
}

impl Mp4InputBuilder {
    pub fn new() -> Self {
        let suffix = rand::thread_rng().next_u32();
        let name = format!("mp4_input_{suffix}");
        Self {
            name,
            source: None,
            input_loop: false,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_source()?.prompt_loop()
    }

    fn prompt_source(self) -> Result<Self> {
        let env_source = env::var(MP4_INPUT_SOURCE).unwrap_or_default();
        let default_path = examples_root_dir().join(BUNNY_H264_PATH);

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
                            break Ok(self.with_source(source))
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

    fn prompt_loop(self) -> Result<Self> {
        let loop_selection = Confirm::new("Loop input [y/n]:").prompt_skippable()?;
        match loop_selection {
            Some(r#loop) => Ok(self.with_loop(r#loop)),
            None => Ok(self),
        }
    }

    pub fn with_source(mut self, source: Mp4InputSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_loop(mut self, r#loop: bool) -> Self {
        self.input_loop = r#loop;
        self
    }

    pub fn build(self) -> Mp4Input {
        Mp4Input {
            name: self.name,
            source: self.source.unwrap(),
            input_loop: self.input_loop,
        }
    }
}
