use anyhow::{anyhow, Result};
use integration_tests::ffmpeg::start_ffmpeg_rtmp_receive;
use rand::RngCore;
use std::process::Child;

use inquire::{Confirm, Select};
use serde_json::json;
use strum::Display;
use tracing::error;

use crate::{
    inputs::InputHandler,
    outputs::{AudioEncoder, OutputHandler, VideoEncoder, VideoResolution},
    players::OutputPlayer,
};

use crate::utils::get_free_port;

#[derive(Debug, Display, Clone)]
pub enum RtmpRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

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
        let player_handle = start_ffmpeg_rtmp_receive(self.port)?;
        self.stream_handles.push(player_handle);
        Ok(())
    }
}

impl OutputHandler for RtmpOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_update(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
        json!({
            "video": self.video.as_ref().map(|v| v.serialize_update(inputs, &self.name)),
            "audio": self.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }

    fn on_before_registration(&mut self, player: OutputPlayer) -> Result<()> {
        match player {
            OutputPlayer::FfmpegReceiver => self.start_ffmpeg_recv(),
            OutputPlayer::Manual => {
                let cmd = format!("ffmpeg -f flv -listen 1 -i 'rtmp://0.0.0.0:{}' -vcodec copy -f flv - | ffplay -autoexit -f flv -i -", self.port);

                println!("Start player: {cmd}");

                loop {
                    let confirmation = Confirm::new("Is player running? [y/n]").prompt()?;
                    if confirmation {
                        return Ok(());
                    }
                }
            }
            _ => Err(anyhow!("Invalid player for RTMP output!")),
        }
    }
}

impl Drop for RtmpOutput {
    fn drop(&mut self) {
        for stream_process in &mut self.stream_handles {
            match stream_process.kill() {
                Ok(_) => {}
                Err(e) => error!("{e}"),
            }
        }
    }
}

pub struct RtmpOutputBuilder {
    name: String,
    url: String,
    port: u16,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
    player: OutputPlayer,
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
            player: OutputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        let video_options = vec![
            RtmpRegisterOptions::SetVideoStream,
            RtmpRegisterOptions::Skip,
        ];
        let audio_options = vec![
            RtmpRegisterOptions::SetAudioStream,
            RtmpRegisterOptions::Skip,
        ];

        loop {
            let video_selection =
                Select::new("Set video stream?", video_options.clone()).prompt_skippable()?;

            builder = match video_selection {
                Some(RtmpRegisterOptions::SetVideoStream) => {
                    builder.with_video(RtmpOutputVideoOptions::default())
                }
                Some(RtmpRegisterOptions::Skip) | None => builder,
                _ => unreachable!(),
            };

            let audio_selection =
                Select::new("Set audio stream?", audio_options.clone()).prompt_skippable()?;

            builder = match audio_selection {
                Some(RtmpRegisterOptions::SetAudioStream) => {
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

        let player_options = vec![OutputPlayer::FfmpegReceiver, OutputPlayer::Manual];
        let player_choice = Select::new("Select player:", player_options).prompt_skippable()?;
        builder = match player_choice {
            Some(player) => builder.with_player(player),
            None => builder,
        };
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

    pub fn with_player(mut self, player: OutputPlayer) -> Self {
        self.player = player;
        self
    }

    fn serialize(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
        json!({
            "type": "rtmp_client",
            "url": self.url,
            "video": self.video.as_ref().map(|v| v.serialize_register(inputs, &self.name)),
            "audio": self.audio.as_ref().map(|a| a.serialize_register(inputs)),
        })
    }

    pub fn build(
        self,
        inputs: &[&dyn InputHandler],
    ) -> (RtmpOutput, serde_json::Value, OutputPlayer) {
        let register_request = self.serialize(inputs);
        let rtmp_output = RtmpOutput {
            name: self.name,
            port: self.port,
            video: self.video,
            audio: self.audio,
            stream_handles: vec![],
        };
        (rtmp_output, register_request, self.player)
    }
}

#[derive(Debug)]
pub struct RtmpOutputVideoOptions {
    root_id: String,
    resolution: VideoResolution,
    encoder: VideoEncoder,
}

impl RtmpOutputVideoOptions {
    pub fn serialize_register(
        &self,
        inputs: &[&dyn InputHandler],
        output_name: &str,
    ) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .filter_map(|input| {
                if input.has_video() {
                    let input_name = input.name();
                    let id = format!("{input_name}_{output_name}");
                    Some(json!({
                        "type": "input_stream",
                        "id": id,
                        "input_id": input_name,
                    }))
                } else {
                    None
                }
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
                    "id": self.root_id,
                    "transition": {
                        "duration_ms": 500,
                    },
                    "children": input_json,
                },
            }
        })
    }

    pub fn serialize_update(
        &self,
        inputs: &[&dyn InputHandler],
        output_name: &str,
    ) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .filter_map(|input| {
                if input.has_video() {
                    let input_name = input.name();
                    let id = format!("{input_name}_{output_name}");
                    Some(json!({
                        "type": "input_stream",
                        "id": id,
                        "input_id": input_name,
                    }))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        json!({
            "root": {
                "type": "tiles",
                "id": self.root_id,
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
        let suffix = rand::thread_rng().next_u32();
        let root_id = format!("tiles_{suffix}");
        Self {
            root_id,
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
    pub fn serialize_register(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .filter_map(|input| {
                if input.has_audio() {
                    Some(json!({
                        "input_id": input.name(),
                    }))
                } else {
                    None
                }
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

    pub fn serialize_update(&self, inputs: &[&dyn InputHandler]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .filter_map(|input| {
                if input.has_audio() {
                    Some(json!({
                        "input_id": input.name(),
                    }))
                } else {
                    None
                }
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
