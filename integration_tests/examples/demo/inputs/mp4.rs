use std::{env, path::PathBuf};

use anyhow::Result;
use inquire::Text;
use integration_tests::{
    assets::{BUNNY_H264_PATH, BUNNY_H264_URL},
    examples::{download_asset, examples_root_dir, AssetData},
};
use rand::RngCore;
use serde_json::json;
use tracing::{error, info};

use crate::{
    autocompletion::FilePathCompleter, inputs::InputHandler, players::InputPlayer,
    utils::resolve_path,
};

const MP4_INPUT_PATH: &str = "MP4_INPUT_PATH";

#[derive(Debug)]
pub struct Mp4Input {
    name: String,
}

impl InputHandler for Mp4Input {
    fn name(&self) -> &str {
        &self.name
    }
}

pub struct Mp4InputBuilder {
    name: String,
    path: Option<PathBuf>,
}

impl Mp4InputBuilder {
    pub fn new() -> Self {
        let suffix = rand::thread_rng().next_u32();
        let name = format!("mp4_input_{suffix}");
        Self { name, path: None }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;
        let env_path = env::var(MP4_INPUT_PATH).unwrap_or_default();
        let default_path = examples_root_dir().join(BUNNY_H264_PATH);

        builder = loop {
            let path_input = Text::new(&format!(
                "Input path (ESC for {}):",
                default_path.to_str().unwrap(),
            ))
            .with_initial_value(&env_path)
            .with_autocomplete(FilePathCompleter::files())
            .prompt_skippable()?;

            match path_input {
                Some(path) if !path.trim().is_empty() => {
                    let path = resolve_path(path.into())?;
                    if path.exists() {
                        break builder.with_path(path);
                    } else {
                        error!("Path is not valid");
                    }
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
                    break builder.with_path(default_path);
                }
            };
        };

        Ok(builder)
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    fn serialize(&self) -> serde_json::Value {
        json!({
            "type": "mp4",
            "path": self.path.as_ref().unwrap(),
        })
    }

    pub fn build(self) -> (Mp4Input, serde_json::Value, InputPlayer) {
        let register_request = self.serialize();

        let mp4_input = Mp4Input { name: self.name };

        (mp4_input, register_request, InputPlayer::Manual)
    }
}
