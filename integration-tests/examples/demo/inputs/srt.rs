use std::{
    env,
    path::PathBuf,
    sync::{OnceLock, atomic::AtomicU32},
};

use anyhow::Result;
use inquire::{Select, Text};
use integration_tests::{assets::BUNNY_H264_PATH, paths::integration_tests_root};
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

use crate::{
    autocompletion::FilePathCompleter,
    inputs::VideoDecoder,
    utils::{get_free_port, resolve_path},
};

const SRT_INPUT_PATH: &str = "SRT_INPUT_PATH";

#[derive(Debug, Display, EnumIter, Clone)]
pub enum SrtRegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Deserialize)]
#[serde(from = "SrtInputOptions")]
pub struct SrtInput {
    pub name: String,
    port: u16,
    options: SrtInputOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtInputOptions {
    video: Option<SrtInputVideoOptions>,
    audio: Option<bool>,
    path: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SrtInputVideoOptions {
    pub decoder: VideoDecoder,
}

impl SrtInputVideoOptions {
    pub fn serialize(&self) -> serde_json::Value {
        json!({
            "decoder": self.decoder.to_string(),
        })
    }
}

impl Default for SrtInputVideoOptions {
    fn default() -> Self {
        Self {
            decoder: VideoDecoder::FfmpegH264,
        }
    }
}

impl Serialize for SrtInput {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("SrtInput", 3)?;
        state.serialize_field("video", &self.options.video)?;
        state.serialize_field("audio", &self.options.audio)?;
        state.serialize_field("path", &self.options.path)?;
        state.end()
    }
}

impl From<SrtInputOptions> for SrtInput {
    fn from(value: SrtInputOptions) -> Self {
        let port = get_free_port();
        let name = format!("srt_input_{port}");
        Self {
            name,
            port,
            options: value,
        }
    }
}

impl SrtInput {
    pub fn serialize_register(&self) -> serde_json::Value {
        let SrtInputOptions { video, audio, .. } = &self.options;
        json!({
            "type": "srt",
            "port": self.port,
            "video": video.as_ref().map(|v| v.serialize()),
            "audio": audio,
        })
    }

    pub fn has_video(&self) -> bool {
        self.options.video.is_some()
    }

    pub fn has_audio(&self) -> bool {
        self.options.audio.unwrap_or(false)
    }

    pub fn on_after_registration(&self) -> Result<()> {
        let input_path = match &self.options.path {
            Some(path) => path.to_str().unwrap().to_string(),
            None => integration_tests_root()
                .join(BUNNY_H264_PATH)
                .to_str()
                .unwrap()
                .to_string(),
        };

        let has_video = self.has_video();
        let has_audio = self.has_audio();

        let video_args = if has_video {
            "-c:v libx264 -tune zerolatency "
        } else {
            "-vn "
        };
        let audio_args = if has_audio { "-c:a aac -ac 2 " } else { "-an " };

        let cmd = format!(
            "ffmpeg -re -i {input_path} {video_args}{audio_args}-f mpegts 'srt://127.0.0.1:{}?mode=caller&pkt_size=1316'",
            self.port,
        );

        println!("Start streaming MPEG-TS over SRT to Smelter:");
        println!("{cmd}");
        println!();

        Ok(())
    }
}

pub struct SrtInputBuilder {
    name: String,
    port: u16,
    video: Option<SrtInputVideoOptions>,
    audio: Option<bool>,
    path: Option<PathBuf>,
}

impl SrtInputBuilder {
    pub fn new() -> Self {
        let port = get_free_port();
        let name = Self::generate_name(port);
        Self {
            name,
            port,
            video: None,
            audio: None,
            path: None,
        }
    }

    fn generate_name(port: u16) -> String {
        static LAST_INPUT: OnceLock<AtomicU32> = OnceLock::new();
        let atomic_suffix = LAST_INPUT.get_or_init(|| AtomicU32::new(0));
        let suffix = atomic_suffix.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        format!("input_srt_{suffix}_{port}")
    }

    pub fn prompt(self) -> Result<Self> {
        let mut builder = self.prompt_path()?;

        loop {
            builder = builder.prompt_video()?.prompt_audio()?;

            if builder.video.is_none() && builder.audio.is_none() {
                error!("Either audio or video stream has to be specified.");
            } else {
                break;
            }
        }

        Ok(builder)
    }

    fn prompt_path(self) -> Result<Self> {
        let env_path = env::var(SRT_INPUT_PATH).unwrap_or_default();
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

    fn prompt_video(self) -> Result<Self> {
        let video_options = vec![SrtRegisterOptions::SetVideoStream, SrtRegisterOptions::Skip];

        let video_selection = Select::new("Set video stream?", video_options).prompt_skippable()?;

        match video_selection {
            Some(SrtRegisterOptions::SetVideoStream) => {
                let codec_options: Vec<VideoDecoder> = VideoDecoder::iter()
                    .filter(|dec| {
                        matches!(dec, VideoDecoder::FfmpegH264 | VideoDecoder::VulkanH264)
                    })
                    .collect();

                let codec_choice =
                    Select::new("Select video codec:", codec_options).prompt_skippable()?;

                match codec_choice {
                    Some(codec) => Ok(self.with_video(SrtInputVideoOptions { decoder: codec })),
                    None => Ok(self.with_video(SrtInputVideoOptions::default())),
                }
            }
            Some(_) | None => Ok(self),
        }
    }

    fn prompt_audio(self) -> Result<Self> {
        let audio_options = vec![SrtRegisterOptions::SetAudioStream, SrtRegisterOptions::Skip];
        let audio_selection =
            Select::new("Set audio stream (AAC)?", audio_options).prompt_skippable()?;

        match audio_selection {
            Some(SrtRegisterOptions::SetAudioStream) => Ok(self.with_audio(true)),
            Some(_) | None => Ok(self),
        }
    }

    pub fn with_video(mut self, video: SrtInputVideoOptions) -> Self {
        self.video = Some(video);
        self
    }

    pub fn with_audio(mut self, audio: bool) -> Self {
        self.audio = Some(audio);
        self
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn build(self) -> SrtInput {
        let options = SrtInputOptions {
            path: self.path,
            video: self.video,
            audio: self.audio,
        };
        SrtInput {
            name: self.name,
            port: self.port,
            options,
        }
    }
}
