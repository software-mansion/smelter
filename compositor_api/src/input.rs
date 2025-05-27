use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod decklink;
mod decklink_into;
mod mp4;
mod mp4_into;
mod rtp;
mod rtp_into;
mod whip;
mod whip_into;

pub use decklink::*;
pub use mp4::*;
pub use rtp::*;
pub use whip::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum VideoDecoder {
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

    /// Deprected
    VulkanVideo,
}

#[cfg(not(feature = "vk-video"))]
const NO_VULKAN_VIDEO: &str =
    "Requested `vulkan_h264` decoder, but this binary was compiled without the `vk-video` feature.";
