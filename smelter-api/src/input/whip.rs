use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::SideChannel;

/// Parameters for an input stream for WHIP server.
/// At least one of `video` and `audio` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipInput {
    /// Parameters of a video source included in the RTP stream.
    pub video: Option<InputWhipVideoOptions>,
    /// Token used for authentication in WHIP protocol. If not provided, the random value
    /// will be generated and returned in the response.
    pub bearer_token: Option<Arc<str>>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Minimum and starting size of the jitter buffer in milliseconds. The buffer
    /// adapts dynamically based on observed network jitter but will not shrink
    /// below this value. Higher values trade latency for resilience.
    pub buffer_size_ms: Option<f64>,
    /// Enable side channel for video and/or audio track.
    pub side_channel: Option<SideChannel>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputWhipVideoOptions {
    pub decoder_preferences: Option<Vec<WhipVideoDecoderOptions>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipVideoDecoderOptions {
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Software VP8 decoder based on FFmpeg.
    FfmpegVp8,

    /// Software VP9 decoder based on FFmpeg.
    FfmpegVp9,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires gpu-video feature.
    VulkanH264,

    Any,
}
