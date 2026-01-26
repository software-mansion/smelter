use std::{env, path::PathBuf, process::Child};

use anyhow::Result;
use inquire::{Select, Text};
use integration_tests::{
    assets::{BUNNY_H264_PATH, BUNNY_H264_URL},
    examples::{AssetData, download_asset},
    ffmpeg::start_ffmpeg_rtmp_send,
    paths::integration_tests_root,
};
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use serde_json::json;
use tracing::error;

use crate::{autocompletion::FilePathCompleter, players::InputPlayer, utils::resolve_path};

use crate::utils::get_free_port;

const RTMP_INPUT_PATH: &str = "RTMP_INPUT_PATH";

#[derive(Debug, Deserialize)]
#[serde(from = "RtmpInputOptions")]
pub struct RtmpInput {
    pub name: String,
    port: u16,
    options: RtmpInputOptions,
    stream_handles: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtmpInputOptions {
    path: Option<PathBuf>,
    player: InputPlayer,
}

impl Serialize for RtmpInput {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("RtmpInput", 2)?;
        state.serialize_field("path", &self.options.path)?;
        state.serialize_field("player", &self.options.player)?;
        state.end()
    }
}

impl From<RtmpInputOptions> for RtmpInput {
    fn from(value: RtmpInputOptions) -> Self {
        let port = get_free_port();
        let name = format!("rtmp_input_{port}");
        Self {
            name,
            port,
            options: value,
            stream_handles: vec![],
        }
    }
}

impl RtmpInput {
    pub fn serialize_register(&self) -> serde_json::Value {
        json!({
            "type": "rtmp_server",
            "url": format!("rtmp://127.0.0.1:{}", self.port),
        })
    }

    pub fn has_video(&self) -> bool {
        true
    }

    fn download_asset(&self) -> Result<PathBuf> {
        let asset = AssetData {
            url: String::from(BUNNY_H264_URL),
            path: integration_tests_root().join(BUNNY_H264_PATH),
        };

        download_asset(&asset)?;
        Ok(asset.path)
    }

    fn ffmpeg_transmit(&mut self) -> Result<()> {
        let handle = match &self.options.path {
            Some(path) => start_ffmpeg_rtmp_send(self.port, path)?,
            None => {
                let asset_path = self.download_asset()?;
                start_ffmpeg_rtmp_send(self.port, &asset_path)?
            }
        };
        self.stream_handles.push(handle);
        Ok(())
    }

    pub fn on_after_registration(&mut self) -> Result<()> {
        let RtmpInputOptions { player, .. } = self.options;
        match player {
            InputPlayer::Ffmpeg => self.ffmpeg_transmit(),
            InputPlayer::Gstreamer => unimplemented!(),
            InputPlayer::Manual => {
                unimplemented!()
            }
        }
    }
}

impl Drop for RtmpInput {
    fn drop(&mut self) {
        for stream_process in &mut self.stream_handles {
            match stream_process.kill() {
                Ok(_) => {}
                Err(e) => error!("{e}"),
            }
        }
    }
}

pub struct RtmpInputBuilder {
    name: String,
    port: u16,
    path: Option<PathBuf>,
    player: InputPlayer,
}

impl RtmpInputBuilder {
    pub fn new() -> Self {
        let port = get_free_port();
        let name = format!("input_rtmp_{port}");
        Self {
            name,
            port,
            path: None,
            player: InputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        self.prompt_path()?.prompt_player()
    }

    fn prompt_path(self) -> Result<Self> {
        let env_path = env::var(RTMP_INPUT_PATH).unwrap_or_default();
        let default_path = integration_tests_root().join(BUNNY_H264_PATH);

        loop {
            let path_input = Text::new(&format!(
                "Input path (ESC for {}):",
                default_path.to_str().unwrap(),
            ))
            .with_autocomplete(FilePathCompleter::default())
            .with_initial_value(&env_path)
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
                Some(_) | None => break Ok(self),
            }
        }
    }

    fn prompt_player(self) -> Result<Self> {
        let player_options = vec![InputPlayer::Ffmpeg, InputPlayer::Manual];
        let player_selection =
            Select::new("Select player (ESC for FFmpeg):", player_options).prompt_skippable()?;
        match player_selection {
            Some(player) => Ok(self.with_player(player)),
            None => Ok(self.with_player(InputPlayer::Ffmpeg)),
        }
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_player(mut self, player: InputPlayer) -> Self {
        self.player = player;
        self
    }

    pub fn build(self) -> RtmpInput {
        let options = RtmpInputOptions {
            path: self.path,
            player: self.player,
        };
        RtmpInput {
            name: self.name,
            port: self.port,
            options,
            stream_handles: vec![],
        }
    }
}
