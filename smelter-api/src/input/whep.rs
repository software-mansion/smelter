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
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Software VP8 decoder based on FFmpeg.
    FfmpegVp8,

    /// Software VP9 decoder based on FFmpeg.
    FfmpegVp9,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires vk-video feature.
    VulkanH264,

    Any,
}
