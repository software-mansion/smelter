use std::{path::Path, sync::Arc};

use serde::Serialize;
use utoipa::ToSchema;

use crate::common_core::prelude as core;

/// Response for requests that do not return any data.
#[derive(Debug, Serialize, ToSchema)]
pub struct OkResponse {}

#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum RegisterInputResponse {
    Rtp {
        port: Option<u16>,
    },
    Mp4 {
        video_duration_ms: Option<u64>,
        audio_duration_ms: Option<u64>,
    },
    Whip {
        bearer_token: Arc<str>,
        endpoint_route: Arc<str>,
    },
    Other {},
}

impl From<core::InputInitInfo> for RegisterInputResponse {
    fn from(value: core::InputInitInfo) -> Self {
        match value {
            core::InputInitInfo::Rtp { port } => Self::Rtp {
                port: port.map(|p| p.0),
            },
            core::InputInitInfo::Mp4 {
                video_duration,
                audio_duration,
            } => Self::Mp4 {
                video_duration_ms: video_duration.map(|v| v.as_millis() as u64),
                audio_duration_ms: audio_duration.map(|a| a.as_millis() as u64),
            },
            core::InputInitInfo::Whip {
                bearer_token,
                endpoint_route,
            } => Self::Whip {
                bearer_token,
                endpoint_route,
            },
            core::InputInitInfo::Other => Self::Other {},
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterOutputResponse {
    /// Port allocated for the output. Only returned for RTP outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl From<Option<core::Port>> for RegisterOutputResponse {
    fn from(value: Option<core::Port>) -> Self {
        Self {
            port: value.map(|p| p.0),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InstanceStatus {
    pub instance_id: String,
    pub configuration: InstanceConfiguration,
    pub inputs: Vec<InputStatus>,
    pub outputs: Vec<OutputStatus>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InstanceConfiguration {
    pub api_port: u16,

    pub output_framerate: f64,
    pub mixing_sample_rate: u32,

    pub ahead_of_time_processing: bool,
    pub never_drop_output_frames: bool,
    pub run_late_scheduled_events: bool,

    #[schema(value_type = str)]
    pub download_root: Arc<Path>,

    pub web_renderer_enable: bool,
    pub web_renderer_gpu_enable: bool,

    pub whip_whep_server_port: u16,
    pub whip_whep_enable: bool,
    pub webrtc_stun_servers: Arc<Vec<String>>,

    pub rendering_mode: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InputStatus {
    pub input_id: String,
    pub input_type: InputType,
}

/// Same values as `type` in the register input request. `raw_data` is used for inputs
/// registered through the Rust API.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    RtpStream,
    RtmpServer,
    MoqServer,
    MoqClient,
    Mp4,
    WhipServer,
    WhepClient,
    Hls,
    V4l2,
    #[serde(rename = "decklink")]
    DeckLink,
    RawData,
}

impl From<core::InputProtocolKind> for InputType {
    fn from(value: core::InputProtocolKind) -> Self {
        match value {
            core::InputProtocolKind::Rtp => Self::RtpStream,
            core::InputProtocolKind::Rtmp => Self::RtmpServer,
            core::InputProtocolKind::MoqServer => Self::MoqServer,
            core::InputProtocolKind::MoqClient => Self::MoqClient,
            core::InputProtocolKind::Mp4 => Self::Mp4,
            core::InputProtocolKind::Whip => Self::WhipServer,
            core::InputProtocolKind::Whep => Self::WhepClient,
            core::InputProtocolKind::Hls => Self::Hls,
            core::InputProtocolKind::V4l2 => Self::V4l2,
            core::InputProtocolKind::DeckLink => Self::DeckLink,
            core::InputProtocolKind::RawDataChannel => Self::RawData,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OutputStatus {
    pub output_id: String,
    pub output_type: OutputType,
}

/// Same values as `type` in the register output request. `encoded_data` and `raw_data`
/// are used for outputs registered through the Rust API.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    RtpStream,
    RtmpClient,
    MoqClient,
    Mp4,
    WhipClient,
    WhepServer,
    Hls,
    EncodedData,
    RawData,
}

impl From<core::OutputProtocolKind> for OutputType {
    fn from(value: core::OutputProtocolKind) -> Self {
        match value {
            core::OutputProtocolKind::Rtp => Self::RtpStream,
            core::OutputProtocolKind::Rtmp => Self::RtmpClient,
            core::OutputProtocolKind::MoqClient => Self::MoqClient,
            core::OutputProtocolKind::Mp4 => Self::Mp4,
            core::OutputProtocolKind::Whip => Self::WhipClient,
            core::OutputProtocolKind::Whep => Self::WhepServer,
            core::OutputProtocolKind::Hls => Self::Hls,
            core::OutputProtocolKind::EncodedDataChannel => Self::EncodedData,
            core::OutputProtocolKind::RawDataChannel => Self::RawData,
        }
    }
}
