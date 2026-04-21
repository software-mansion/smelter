use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::SideChannel;

/// Parameters for an input stream received over SRT.
/// Expects an MPEG-TS stream carrying H.264 video and/or AAC audio.
/// At least one of `video` and `audio` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SrtInput {
    /// UDP port on which the compositor listens for incoming SRT connections.
    pub port: u16,
    /// Parameters of the video track carried in the MPEG-TS stream.
    pub video: Option<InputSrtVideoOptions>,
    /// Whether an AAC audio track is present in the MPEG-TS stream.
    pub audio: Option<bool>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If the offset is
    /// not defined then the stream will be synchronized based on the delivery time of the initial
    /// frames.
    pub offset_ms: Option<f64>,
    /// Enable side channel for video and/or audio track.
    pub side_channel: Option<SideChannel>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputSrtVideoOptions {
    pub decoder: SrtVideoDecoderOptions,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SrtVideoDecoderOptions {
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires vk-video feature.
    VulkanH264,
}
