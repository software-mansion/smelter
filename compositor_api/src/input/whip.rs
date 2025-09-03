use core::f64;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common_pipeline::prelude as pipeline;
use crate::*;

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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipVideoDecoderOptions {
    /// Use the software h264 decoder based on ffmpeg.
    FfmpegH264,

    /// Use the software vp8 decoder based on ffmpeg.
    FfmpegVp8,

    /// Use the software vp9 decoder based on ffmpeg.
    FfmpegVp9,

    /// Use hardware decoder based on Vulkan Video.
    ///
    /// This should be faster and more scalable than teh ffmpeg decoder, if the hardware and OS
    /// support it.
    ///
    /// This requires hardware that supports Vulkan Video. Another requirement is this program has
    /// to be compiled with the `vk-video` feature enabled (enabled by default on platforms which
    /// support Vulkan, i.e. non-Apple operating systems and not the web).
    VulkanH264,

    Any,
}

impl From<WhipVideoDecoderOptions> for pipeline::WhipVideoDecoderOptions {
    fn from(decoder: WhipVideoDecoderOptions) -> Self {
        match decoder {
            WhipVideoDecoderOptions::FfmpegH264 => pipeline::WhipVideoDecoderOptions::FfmpegH264,
            WhipVideoDecoderOptions::FfmpegVp8 => pipeline::WhipVideoDecoderOptions::FfmpegVp8,
            WhipVideoDecoderOptions::FfmpegVp9 => pipeline::WhipVideoDecoderOptions::FfmpegVp9,
            WhipVideoDecoderOptions::VulkanH264 => pipeline::WhipVideoDecoderOptions::VulkanH264,
            WhipVideoDecoderOptions::Any => pipeline::WhipVideoDecoderOptions::Any,
        }
    }
}
