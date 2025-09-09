use std::{env, path::PathBuf};

use anyhow::Result;
use inquire::{Select, Text};
use integration_tests::{
    assets::{BUNNY_H264_PATH, BUNNY_H264_URL},
    examples::{download_asset, examples_root_dir, AssetData},
};
use rand::RngCore;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{error, info};

use crate::{
    autocompletion::FilePathCompleter, inputs::InputHandler, players::InputPlayer,
    utils::resolve_path,
};

const MP4_INPUT_PATH: &str = "MP4_INPUT_PATH";

#[derive(Debug, Display, EnumIter)]
enum Mp4RegisterOptions {
    #[strum(to_string = "Loop")]
    Loop,

    #[strum(to_string = "Skip")]
    Skip,
}

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
    r#loop: bool,
}

impl Mp4InputBuilder {
    pub fn new() -> Self {
        let suffix = rand::thread_rng().next_u32();
        let name = format!("mp4_input_{suffix}");
        Self {
            name,
            path: None,
            r#loop: false,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        builder = builder.prompt_path()?;

        let loop_options = Mp4RegisterOptions::iter().collect::<Vec<_>>();
        let loop_selection = Select::new("Set loop:", loop_options).prompt_skippable()?;
        builder = match loop_selection {
            Some(Mp4RegisterOptions::Loop) => builder.with_loop(true),
            Some(_) | None => builder,
        };

        Ok(builder)
    }

    pub fn prompt_path(self) -> Result<Self> {
        let env_path = env::var(MP4_INPUT_PATH).unwrap_or_default();
        let default_path = examples_root_dir().join(BUNNY_H264_PATH);

        loop {
            let path_input = Text::new(&format!(
                "Input path (ESC for {}):",
                default_path.to_str().unwrap(),
            ))
            .with_initial_value(&env_path)
            .with_autocomplete(FilePathCompleter::default())
            .prompt_skippable()?;

            match path_input {
                Some(path) if !path.trim().is_empty() => {
                    let path = resolve_path(path.into())?;
                    if path.exists() {
                        break Ok(self.with_path(path));
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
                    break Ok(self.with_path(default_path));
                }
            }
        }
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_loop(mut self, r#loop: bool) -> Self {
        self.r#loop = r#loop;
        self
    }

    fn serialize(&self) -> serde_json::Value {
        json!({
            "type": "mp4",
            "path": self.path.as_ref().unwrap(),
            "loop": self.r#loop,
        })
    }

    pub fn build(self) -> (Mp4Input, serde_json::Value, InputPlayer) {
        let register_request = self.serialize();

        let mp4_input = Mp4Input { name: self.name };

        (mp4_input, register_request, InputPlayer::Manual)
    }
}
