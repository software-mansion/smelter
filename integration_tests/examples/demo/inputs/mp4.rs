use std::{env, fs};

use anyhow::Result;
use inquire::Text;
use rand::RngCore;
use serde_json::json;
use tracing::{error, info};

use crate::{inputs::InputHandler, players::InputPlayer};

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
    path: Option<String>,
}

impl Mp4InputBuilder {
    pub fn new() -> Self {
        let suffix = rand::thread_rng().next_u32();
        let name = format!("mp4_input_{suffix}");
        Self { name, path: None }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        loop {
            let path_input = Text::new(&format!(
                "Absolute input path (ESC for env {MP4_INPUT_PATH}):"
            ))
            .prompt_skippable()?;

            builder = match path_input {
                Some(path) if fs::exists(&path).unwrap_or(false) => builder.with_path(path),
                Some(path) => {
                    error!("{path} does not exist");
                    continue;
                }
                None => match env::var(MP4_INPUT_PATH).ok() {
                    Some(path) if fs::exists(&path).unwrap_or(false) => {
                        info!("Path read from env: {path}");
                        builder.with_path(path)
                    }
                    Some(path) => {
                        error!("{path} does not exist");
                        continue;
                    }
                    None => {
                        error!("Env {MP4_INPUT_PATH} not found or invalid. Please enter path manually.");
                        continue;
                    }
                },
            };
            break;
        }

        Ok(builder)
    }

    pub fn with_path(mut self, path: String) -> Self {
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
