use std::{env, path::PathBuf};

use anyhow::Result;
use inquire::{Select, Text};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::autocompletion::FilePathCompleter;

const V4L2_INPUT_PATH: &str = "V4L2_INPUT_PATH";
const DEFAULT_DEVICE_PATH: &str = "/dev/video0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "V4l2InputOptions", into = "V4l2InputOptions")]
pub struct V4l2Input {
    pub name: String,
    options: V4l2InputOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V4l2InputOptions {
    path: PathBuf,
    format: V4l2Format,
}

impl From<V4l2InputOptions> for V4l2Input {
    fn from(value: V4l2InputOptions) -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("v4l2_input_{suffix}");
        Self {
            name,
            options: value,
        }
    }
}

impl From<V4l2Input> for V4l2InputOptions {
    fn from(value: V4l2Input) -> Self {
        value.options
    }
}

impl V4l2Input {
    pub fn serialize_register(&self) -> serde_json::Value {
        let V4l2InputOptions { ref path, format } = self.options;
        json!({
            "type": "v4l2",
            "path": path.to_str().unwrap(),
            "format": format.to_string(),
            "side_channel": {
                "video": true,
            }
        })
    }
}

#[derive(Debug, Display, EnumIter, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum V4l2Format {
    #[strum(to_string = "yuyv")]
    Yuyv,

    #[strum(to_string = "nv12")]
    Nv12,
}

pub struct V4l2InputBuilder {
    name: String,
    path: PathBuf,
    format: V4l2Format,
}

impl V4l2InputBuilder {
    pub fn new() -> Self {
        let suffix = rand::rng().next_u32();
        let name = format!("v4l2_input_{suffix}");
        Self {
            name,
            path: DEFAULT_DEVICE_PATH.into(),
            format: V4l2Format::Yuyv,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_path()?.prompt_format()
    }

    fn prompt_path(self) -> Result<Self> {
        let env_path = env::var(V4L2_INPUT_PATH).unwrap_or_default();

        loop {
            let path_input =
                Text::new(&format!("Device path (ESC for \"{DEFAULT_DEVICE_PATH}\"):"))
                    .with_autocomplete(FilePathCompleter::default())
                    .with_initial_value(&env_path)
                    .prompt_skippable()?;

            match path_input {
                Some(path) if !path.trim().is_empty() => {
                    let path = PathBuf::from(path.trim());
                    if !path.exists() {
                        error!("Path is not valid");
                        continue;
                    }
                    return Ok(self.with_path(path));
                }
                Some(_) | None => return Ok(self),
            }
        }
    }

    fn prompt_format(self) -> Result<Self> {
        let format_options = V4l2Format::iter().collect();
        let format_selection =
            Select::new("Select format: (ESC for yuyv)", format_options).prompt_skippable()?;

        match format_selection {
            Some(format) => Ok(self.with_format(format)),
            None => Ok(self),
        }
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = path;
        self
    }

    pub fn with_format(mut self, format: V4l2Format) -> Self {
        self.format = format;
        self
    }

    pub fn build(self) -> V4l2Input {
        let options = V4l2InputOptions {
            path: self.path,
            format: self.format,
        };
        V4l2Input {
            name: self.name,
            options,
        }
    }
}
