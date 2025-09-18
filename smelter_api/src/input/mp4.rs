use core::f64;
use std::{collections::HashMap, path::Path, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Input stream from MP4 file.
/// Exactly one of `url` and `path` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mp4Input {
    /// URL of the MP4 file.
    pub url: Option<Arc<str>>,
    /// Path to the MP4 file.
    pub path: Option<Arc<Path>>,
    /// (**default=`false`**) If input should be played in the loop. <span class="badge badge--primary">Added in v0.4.0</span>
    #[serde(rename = "loop")]
    pub should_loop: Option<bool>,
    /// (**default=`false`**) If input is required and frames are not processed
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If offset is
    /// not defined then stream is synchronized based on the first frames delivery time.
    pub offset_ms: Option<f64>,
    /// Assigns which decoder should be used for media encoded with a specific codec.
    pub decoder_map: Option<HashMap<InputMp4Codec, Mp4VideoDecoderOptions>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InputMp4Codec {
    H264,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Mp4VideoDecoderOptions {
    /// Use the software h264 decoder based on ffmpeg.
    FfmpegH264,

    /// Use hardware decoder based on Vulkan Video.
    ///
    /// This should be faster and more scalable than teh ffmpeg decoder, if the hardware and OS
    /// support it.
    ///
    /// This requires hardware that supports Vulkan Video. Another requirement is this program has
    /// to be compiled with the `vk-video` feature enabled (enabled by default on platforms which
    /// support Vulkan, i.e. non-Apple operating systems and not the web).
    VulkanH264,
}
