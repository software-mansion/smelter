use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Yuv420p,
    Yuv422p,
    Yuv444p,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema)]
#[serde(untagged)]
pub enum VideoEncoderBitrate {
    /// Average bitrate measured in bits/second. Encoder will try to keep the bitrate around the
    /// provided average, but may temporarily increase it by 25%.
    AverageBitrate(u64),
    Vbr {
        /// Average bitrate measured in bits/second. Encoder will try to keep the bitrate around the
        /// provided average, but may temporarily increase it to the provided max bitrate.
        average_bitrate: u64,
        /// Max bitrate measured in bits/second.
        max_bitrate: u64,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpusEncoderPreset {
    /// Best for broadcast/high-fidelity application where the decoded audio
    /// should be as close as possible to the input.
    Quality,
    /// Best for most VoIP/videoconference applications where listening quality
    /// and intelligibility matter most.
    Voip,
    /// Only use when lowest-achievable latency is what matters most.
    LowestLatency,
}

pub const NO_VULKAN_VIDEO: &str = "Requested `vulkan_h264` encoder, but this binary was compiled without the `gpu-video` feature.";
