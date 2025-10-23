use core::f64;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for an input stream for WHIP server.
/// At least one of `video` and `audio` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipInput {
    /// Parameters of a video source included in the RTP stream.
    pub video: Option<InputWhipVideoOptions>,
    /// Token used for authentication in WHIP protocol. If not provided, the random value
    /// will be generated and returned in the response.
    pub bearer_token: Option<Arc<str>>,
    /// Internal use only.
    /// Overrides whip endpoint id which is used when referencing the input via whip server.
    /// If not provided, it defaults to input id.
    pub endpoint_override: Option<Arc<str>>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If the offset is
    /// not defined then the stream will be synchronized based on the delivery time of the initial
    /// frames.
    pub offset_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputWhipVideoOptions {
    pub decoder_preferences: Option<Vec<WhipVideoDecoderOptions>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipVideoDecoderOptions {
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
