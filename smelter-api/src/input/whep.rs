use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhepInput {
    /// WHEP server endpoint URL
    pub endpoint_url: Arc<str>,
    /// Optional Bearer token for auth
    pub bearer_token: Option<Arc<str>>,
    /// Parameters of a video source included in the RTP stream.
    pub video: Option<InputWhepVideoOptions>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request).
    pub offset_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputWhepVideoOptions {
    pub decoder_preferences: Option<Vec<WhepVideoDecoderOptions>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WhepVideoDecoderOptions {
    /// Use the software h264 decoder based on ffmpeg.
    FfmpegH264,

    /// Use the software vp8 decoder based on ffmpeg.
    FfmpegVp8,

    /// Use the software vp9 decoder based on ffmpeg.
    FfmpegVp9,

    /// Use hardware decoder based on Vulkan Video.
    ///
    /// This should be faster and more scalable than the ffmpeg decoder, if the hardware and OS
    /// support it.
    ///
    /// This requires hardware that supports Vulkan Video. Another requirement is this program has
    /// to be compiled with the `vk-video` feature enabled (enabled by default on platforms which
    /// support Vulkan, i.e. non-Apple operating systems and not the web).
    VulkanH264,

    Any,
}
