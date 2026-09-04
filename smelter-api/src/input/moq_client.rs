use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{InputBuffer, SideChannel};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MoqClientInput {
    /// URL of the MoQ relay to connect to. Must use the `https://` scheme.
    pub endpoint_url: Arc<str>,
    /// Path of the broadcast to subscribe to on the relay.
    pub broadcast_path: Arc<str>,
    /// If input is required and the stream is not delivered on time, then Smelter will delay
    /// producing output frames.
    ///
    /// Defaults to `false`.
    pub required: Option<bool>,
    /// Assigns which decoder should be used for media encoded with a specific codec.
    pub decoder_map: Option<HashMap<InputMoqClientCodec, MoqClientVideoDecoderOptions>>,
    /// Enable side channel for video and/or audio track.
    pub side_channel: Option<SideChannel>,
    /// (default=2000) Buffer kept between the live edge of the stream and playback.
    /// A number value represents `buffer.desired_ms` option
    pub buffer: Option<InputBuffer>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InputMoqClientCodec {
    H264,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MoqClientVideoDecoderOptions {
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires gpu-video feature.
    VulkanH264,
}
