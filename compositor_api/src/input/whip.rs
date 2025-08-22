use core::f64;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

/// Parameters for an input stream for WHIP server.
/// At least one of `video` and `audio` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipInput {
    /// Parameters of a video source included in the RTP stream.
    pub video: Option<InputWhipVideoOptions>,
    /// Parameters of an audio source included in the RTP stream.
    pub audio: Option<InputWhipAudioOptions>,
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
#[serde(tag = "decoder", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputWhipAudioOptions {
    Opus {
        /// (**default=`false`**) Specifies whether the stream uses forward error correction.
        /// It's specific for Opus codec.
        /// For more information, check out [RFC](https://datatracker.ietf.org/doc/html/rfc6716#section-2.1.7).
        forward_error_correction: Option<bool>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputWhipVideoOptions {
    pub decoder: Option<VideoDecoder>,
    pub decoder_preferences: Option<Vec<WhipVideoDecoder>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipVideoDecoder {
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
