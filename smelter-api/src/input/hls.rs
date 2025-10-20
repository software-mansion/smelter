use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for an input stream from HLS source.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HlsInput {
    /// URL to HLS playlist
    pub url: Arc<str>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If the offset is
    /// not defined then the stream will be synchronized based on the delivery time of the initial
    /// frames.
    pub offset_ms: Option<f64>,
    /// Assigns which decoder should be used for media encoded with a specific codec.
    pub decoder_map: Option<HashMap<InputHlsCodec, HlsVideoDecoderOptions>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InputHlsCodec {
    H264,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum HlsVideoDecoderOptions {
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires vk-video feature.
    VulkanH264,
}
