use anyhow::Result;
use integration_tests::ffmpeg::start_ffmpeg_rtmp_receive;
use std::process::Child;

use inquire::Select;
use serde_json::json;
use strum::Display;
use tracing::error;

use crate::smelter_state::{
    get_free_port,
    outputs::{AudioEncoder, OutputHandler, VideoEncoder, VideoResolution},
    players::OutputPlayerOptions,
};

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
    port: u16,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
    stream_handles: Vec<Child>,
}

impl RtmpOutput {
    fn start_ffmpeg_recv(&mut self) -> Result<()> {
        let handle = start_ffmpeg_rtmp_receive(self.port)?;
        self.stream_handles.push(handle);
        Ok(())
    }
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

    fn on_before_registration(&mut self) -> Result<()> {
        let player_options = vec![
            OutputPlayerOptions::StartFfmpegReceiver,
            OutputPlayerOptions::Manual,
        ];

        loop {
            let player_choice = Select::new("Select player:", player_options.clone()).prompt()?;

            let player_result = match player_choice {
                OutputPlayerOptions::StartFfmpegReceiver => self.start_ffmpeg_recv(),
                OutputPlayerOptions::Manual => Ok(()),
                _ => unreachable!(),
            };

            match player_result {
                Ok(_) => break,
                Err(e) => error!("{e}"),
            }
        }
        Ok(())
    }

    fn on_after_registration(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct RtmpOutputBuilder {
    name: String,
    url: String,
    port: u16,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
}

impl RtmpOutputBuilder {
    pub fn new() -> Self {
        let port = get_free_port();
        Self {
            name: format!("output_rtmp_{port}"),
            url: format!("rtmp://127.0.0.1:{port}"),
            port,
            video: None,
            audio: None,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        let video_options = vec![
            RtmpRegisterOptions::AddVideoStream,
            RtmpRegisterOptions::Skip,
        ];
        let audio_options = vec![
            RtmpRegisterOptions::AddAudioStream,
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
            port: self.port,
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
