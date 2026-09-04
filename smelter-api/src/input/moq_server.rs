use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{InputBuffer, SideChannel};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MoqServerInput {
    /// Token used for authentication in MoQ server input. The broadcaster must provide
    /// it as a `token` query parameter when connecting
    pub auth_token: Arc<str>,
    /// If input is required and the stream is not delivered on time, then Smelter will delay
    /// producing output frames.
    ///
    /// Defaults to `false`.
    pub required: Option<bool>,
    /// Assigns which decoder should be used for media encoded with a specific codec.
    pub decoder_map: Option<HashMap<InputMoqServerCodec, MoqServerVideoDecoderOptions>>,
    /// Enable side channel for video and/or audio track.
    pub side_channel: Option<SideChannel>,
    /// (default=2000) Buffer kept between the live edge of the stream and playback.
    /// A number value represents `buffer.desired_ms` option
    pub buffer: Option<InputBuffer>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InputMoqServerCodec {
    H264,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MoqServerVideoDecoderOptions {
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires gpu-video feature.
    VulkanH264,
}
