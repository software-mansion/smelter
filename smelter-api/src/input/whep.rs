use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::SideChannel;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
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
    /// Enable side channel for video and/or audio track.
    pub side_channel: Option<SideChannel>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputWhepVideoOptions {
    pub decoder_preferences: Option<Vec<WhepVideoDecoderOptions>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema)]
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
