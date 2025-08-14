use anyhow::Result;
use std::{env, process::Child};

use inquire::{Select, Text};
use serde_json::json;
use strum::Display;
use tracing::error;

use crate::smelter_state::outputs::{AudioEncoder, OutputHandler, VideoEncoder, VideoResolution};

pub const SMELTER_RTMP_URL: &str = "SMELTER_RTMP_URL";

#[derive(Debug, Display, Clone)]
pub enum RtmpRegisterOptions {
    #[strum(to_string = "Add video stream")]
    AddVideoStream,

    #[strum(to_string = "Add audio stream")]
    AddAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug)]
pub struct RtmpOutput {
    name: String,
    url: String,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
    stream_handles: Vec<Child>,
}

impl OutputHandler for RtmpOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value {
        json!({
            "video": self.video.as_ref().map(|v| v.serialize_update(inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }

    fn on_before_registration(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_after_registration(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct RtmpOutputBuilder {
    name: String,
    url: String,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
}

impl RtmpOutputBuilder {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            url: String::new(),
            video: None,
            audio: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        let url: String = match env::var(SMELTER_RTMP_URL) {
            Ok(url) => url,
            Err(_) => builder.prompt_url()?,
        };

        builder = builder.with_url(url);

        let video_options = vec![
            RtmpRegisterOptions::AddVideoStream,
            RtmpRegisterOptions::Skip,
        ];
        let audio_options = vec![
            RtmpRegisterOptions::AddVideoStream,
            RtmpRegisterOptions::Skip,
        ];

        loop {
            let video_selection =
                Select::new("Add video stream?", video_options.clone()).prompt_skippable()?;

            builder = match video_selection {
                Some(RtmpRegisterOptions::AddVideoStream) => {
                    builder.with_video(RtmpOutputVideoOptions::default())
                }
                Some(RtmpRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            let audio_selection =
                Select::new("Add audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(RtmpRegisterOptions::AddAudioStream) => {
                    builder.with_audio(RtmpOutputAudioOptions::default())
                }
                Some(RtmpRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            if builder.video.is_none() && builder.audio.is_none() {
                error!("At least one video or one audio stream has to be specified!");
            } else {
                break;
            }
        }
        Ok(builder)
    }

    fn prompt_url(&self) -> Result<String> {
        Ok(Text::new("Url:").prompt()?)
    }

    pub fn with_url(mut self, url: String) -> Self {
        let name = format!("output_rtmp_{url}");
        self.name = name;
        self.url = url;
        self
    }

    pub fn with_video(mut self, video: RtmpOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: RtmpOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    fn serialize(&self, inputs: &[&str]) -> serde_json::Value {
        json!({
            "type": "rtmp_client",
            "url": self.url,
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    pub fn build(self, inputs: &[&str]) -> (RtmpOutput, serde_json::Value) {
        let register_request = self.serialize(inputs);
        let rtmp_output = RtmpOutput {
            name: self.name,
            url: self.url,
            video: self.video,
            audio: self.audio,
            stream_handles: vec![],
        };
        (rtmp_output, register_request)
    }
}

#[derive(Debug)]
pub struct RtmpOutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
}

impl RtmpOutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[&str]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "type": "input_stream",
                    "id": input_id,
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "resolution": self.resolution.serialize(),
            "encoder" : {
                "type": self.encoder.to_string(),
            },
            "initial": {
                "root": {
                    "type": "tiles",
                    "id": "tiles",
                    "transition": {
                        "duration_ms": 500,
                    },
                    "children": input_json,
                },
            }
        })
    }

    pub fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "type": "input_stream",
                    "id": input_id,
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "root": {
                "type": "tiles",
                "id": "tiles",
                "transition": {
                    "duration_ms": 500,
                },
                "children": input_json,
            },
        })
    }
}

impl Default for RtmpOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        Self {
            resolution,
            encoder: VideoEncoder::FfmpegH264,
        }
    }
}

#[derive(Debug)]
pub struct RtmpOutputAudioOptions {
    encoder: AudioEncoder,
}

impl RtmpOutputAudioOptions {
    pub fn serialize_register(&self, inputs: &[&str]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "encoder": {
                "type": self.encoder.to_string(),
            },
            "initial": {
                "inputs": input_json,
            }
        })
    }

    pub fn serialize_update(&self, inputs: &[&str]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "inputs": input_json,
        })
    }
}

impl Default for RtmpOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Aac,
        }
    }
}
