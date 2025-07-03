use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for an input stream from HLS source.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HlsInput {
    /// URL to HLS playlist
    pub url: Arc<str>,
    /// (**default=`ffmpeg_h264`**) The decoder to use for decoding video.
    pub video_decoder: Option<HlsVideoDecoder>,
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
pub enum HlsVideoDecoder {
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

    /// Deprected
    VulkanVideo,
}
