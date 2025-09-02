use std::{env, path::PathBuf};

use anyhow::Result;
use inquire::Text;
use integration_tests::examples::{download_asset, examples_root_dir, AssetData};
use rand::RngCore;
use serde_json::json;

use crate::{
    inputs::InputHandler,
    players::InputPlayer,
    utils::{ELEPHANT_PATH, ELEPHANT_URL},
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

        let path_input =
            Text::new("Input path (absolute or relative to 'smelter/integration_tests'):")
                .with_initial_value(&env_path)
                .prompt()?;

        // TODO: Change that do Big Buck Bunny (which is currently not working)
        builder = if path_input.is_empty() {
            let path = examples_root_dir().join(ELEPHANT_PATH);
            let bunny_data = AssetData {
                url: ELEPHANT_URL.to_string(),
                path: path.clone(),
            };
            download_asset(&bunny_data)?;
            builder.with_path(path)
        } else {
            builder.with_path(path_input.into())
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
