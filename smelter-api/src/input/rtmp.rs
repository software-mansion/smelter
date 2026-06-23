use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::SideChannel;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RtmpInput {
    /// The RTMP stream key.
    ///
    /// In most RTMP clients you will need to provide url in following format
    /// `rtmp://<ip_address>:<port>/<input_id>/<stream_key>`
    pub stream_key: Arc<str>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Assigns which decoder should be used for media encoded with a specific codec.
    pub decoder_map: Option<HashMap<InputRtmpCodec, RtmpVideoDecoderOptions>>,
    /// Enable side channel for video and/or audio track.
    pub side_channel: Option<SideChannel>,
}

#[derive(
    Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq, Eq, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum InputRtmpCodec {
    H264,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RtmpVideoDecoderOptions {
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires gpu-video feature.
    VulkanH264,
}
