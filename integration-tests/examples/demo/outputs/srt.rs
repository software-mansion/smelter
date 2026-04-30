use anyhow::Result;
use inquire::Select;
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use serde_json::json;
use strum::{Display, IntoEnumIterator};
use tracing::error;

use crate::{
    inputs::{InputHandle, filter_video_inputs},
    outputs::{VideoEncoder, VideoResolution, scene::Scene},
    players::OutputPlayer,
};

#[derive(Debug, Display, Clone)]
pub enum SrtRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Deserialize)]
#[serde(from = "SrtOutputOptions")]
pub struct SrtOutput {
    pub name: String,
    options: SrtOutputOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtOutputOptions {
    video: Option<SrtOutputVideoOptions>,
    audio: Option<SrtOutputAudioOptions>,
    player: OutputPlayer,
}

impl Serialize for SrtOutput {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("SrtOutput", 3)?;
        state.serialize_field("video", &self.options.video)?;
        state.serialize_field("audio", &self.options.audio)?;
        state.serialize_field("player", &self.options.player)?;
        state.end()
    }
}

impl From<SrtOutputOptions> for SrtOutput {
    fn from(value: SrtOutputOptions) -> Self {
        let name = generate_output_name();
        Self {
            name,
            options: value,
        }
    }
}

fn generate_output_name() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("output_srt_{nanos}")
}

impl SrtOutput {
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        json!({
            "type": "srt",
            "video": self.options.video.as_ref().map(|v| v.serialize_register(inputs)),
            "audio": self.options.audio.as_ref().map(|a| a.serialize_register(inputs)),
            "encryption": {
                "passphrase": "smelter1234",
                "encryption": "aes128",
            },
        })
    }

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
        json!({
            "video": self.options.video.as_ref().map(|v| v.serialize_update(inputs)),
            "audio": self.options.audio.as_ref().map(|a| a.serialize_update(inputs)),
        })
    }

    pub fn on_after_registration(&mut self) -> Result<()> {
        match self.options.player {
            OutputPlayer::Manual => {
                let srt_port =
                    std::env::var("SMELTER_SRT_SERVER_PORT").unwrap_or_else(|_| "9710".to_string());
                let cmd = format!(
                    "ffplay -fflags nobuffer -flags low_delay 'srt://127.0.0.1:{srt_port}?mode=caller&pkt_size=1316&streamid={stream_id}&passphrase=smelter1234&pbkeylen=16'",
                    stream_id = self.name,
                );

                println!("Start SRT caller to receive the MPEG-TS stream:");
                println!("{cmd}");
                println!();

                Ok(())
            }
            _ => unreachable!(),
        }
    }
}

pub struct SrtOutputBuilder {
    name: String,
    video: Option<SrtOutputVideoOptions>,
    audio: Option<SrtOutputAudioOptions>,
    player: OutputPlayer,
}

impl SrtOutputBuilder {
    pub fn new() -> Self {
        let name = generate_output_name();
        Self {
            name,
            video: None,
            audio: None,
            player: OutputPlayer::Manual,
        }
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self;

        loop {
            builder = builder.prompt_video()?.prompt_audio()?;

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either video or audio has to be specified.");
            } else {
                break;
            }
        }

        Ok(builder)
    }

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![SrtRegisterOptions::SetVideoStream, SrtRegisterOptions::Skip];
        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(SrtRegisterOptions::SetVideoStream) => {
                let mut video = SrtOutputVideoOptions::default();

                let scene_options = Scene::iter().collect();
                let scene_choice = Select::new("Select scene (ESC for Tiles):", scene_options)
                    .prompt_skippable()?;
                if let Some(scene) = scene_choice {
                    video.scene = scene;
                }

                let encoder_options = vec![
                    VideoEncoder::FfmpegH264,
                    VideoEncoder::FfmpegH264LowLatency,
                    VideoEncoder::VulkanH264,
                ];
                let encoder_choice =
                    Select::new("Select encoder (ESC for ffmpeg_h264):", encoder_options)
                        .prompt_skippable()?;
                if let Some(encoder) = encoder_choice {
                    video.encoder = encoder;
                }

                Ok(self.with_video(video))
            }
            Some(SrtRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![SrtRegisterOptions::SetAudioStream, SrtRegisterOptions::Skip];
        let audio_selection =
            Select::new("Set audio stream (AAC)?", audio_options).prompt_skippable()?;

        match audio_selection {
            Some(SrtRegisterOptions::SetAudioStream) => {
                Ok(self.with_audio(SrtOutputAudioOptions::default()))
            }
            Some(SrtRegisterOptions::Skip) | None => Ok(self),
            _ => unreachable!(),
        }
    }

    pub fn with_video(mut self, video: SrtOutputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: SrtOutputAudioOptions) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn build(self) -> SrtOutput {
        let options = SrtOutputOptions {
            video: self.video,
            audio: self.audio,
            player: self.player,
        };
        SrtOutput {
            name: self.name,
            options,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SrtOutputVideoOptions {
    root_id: String,
    resolution: VideoResolution,
    encoder: VideoEncoder,
    scene: Scene,
}

impl SrtOutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "resolution": self.resolution.serialize(),
            "encoder": self.encoder.serialize(),
            "initial": {
                "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
            },
        })
    }

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let inputs = filter_video_inputs(inputs);
        json!({
            "root": self.scene.serialize(&self.root_id, &inputs, self.resolution),
        })
    }
}

impl Default for SrtOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        let root_id = "root".to_string();
        Self {
            root_id,
            resolution,
            encoder: VideoEncoder::FfmpegH264,
            scene: Scene::Tiles,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SrtOutputAudioOptions {}

impl SrtOutputAudioOptions {
    pub fn serialize_register(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let inputs_json = inputs
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
                "type": "aac",
            },
            "initial": {
                "inputs": inputs_json,
            }
        })
    }

    pub fn serialize_update(&self, inputs: &[InputHandle]) -> serde_json::Value {
        let inputs_json = inputs
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
            "inputs": inputs_json,
        })
    }
}
