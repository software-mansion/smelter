use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::SideChannel;

/// Parameters for an input stream received over SRT.
/// Expects an MPEG-TS stream carrying H.264 video and/or AAC audio.
/// At least one of `video` and `audio` has to be defined.
///
/// The input id is used as the SRT `streamid`. Senders must provide a matching
/// `streamid=...` query parameter (or `SRTO_STREAMID` socket option) when
/// connecting to the compositor's shared SRT server.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SrtInput {
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
    /// Enable AES encryption on the incoming SRT stream. The sender must
    /// connect with the matching `passphrase`.
    pub encryption: Option<SrtInputEncryption>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SrtInputEncryption {
    /// Passphrase used to derive the AES key. Must be 10–79 characters long.
    pub passphrase: Arc<str>,
    /// AES key length used for the stream.
    pub encryption: SrtEncryption,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SrtEncryption {
    Aes128,
    Aes192,
    Aes256,
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
