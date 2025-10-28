use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mp4Output {
    /// Path to output MP4 file.
    pub path: String,
    /// Video track configuration.
    pub video: Option<OutputVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputMp4AudioOptions>,
    /// Raw FFmpeg muxer options. See [docs](https://ffmpeg.org/ffmpeg-formats.html) for more.
    pub ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputMp4AudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: Mp4AudioEncoderOptions,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mp4AudioEncoderOptions {
    Aac {
        /// (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
        sample_rate: Option<u32>,
    },
}
