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
    pub inputs: Vec<InputInfo>,
    pub outputs: Vec<OutputInfo>,
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
    pub web_renderer_enable_gpu: bool,

    pub whip_whep_server_port: u16,
    pub whip_whep_enable: bool,
    pub webrtc_stun_servers: Arc<Vec<String>>,

    pub rendering_mode: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InputInfo {
    pub input_id: String,
    pub input_type: String,
}

impl InputInfo {
    pub fn new(input_id: String, protocol: core::InputProtocolKind) -> Self {
        let input_type = match protocol {
            core::InputProtocolKind::Rtp => "rtp",
            core::InputProtocolKind::Rtmp => "rtmp",
            core::InputProtocolKind::Mp4 => "mp4",
            core::InputProtocolKind::Whip => "whip",
            core::InputProtocolKind::Whep => "whep",
            core::InputProtocolKind::Hls => "hls",
            core::InputProtocolKind::MoqServer => "moq_server",
            core::InputProtocolKind::MoqClient => "moq_client",
            core::InputProtocolKind::V4l2 => "v4l2",
            core::InputProtocolKind::DeckLink => "decklink",
            core::InputProtocolKind::RawDataChannel => "raw_data",
        };
        Self {
            input_id,
            input_type: input_type.to_string(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OutputInfo {
    pub output_id: String,
    pub output_type: String,
}

impl OutputInfo {
    pub fn new(output_id: String, protocol: core::OutputProtocolKind) -> Self {
        let output_type = match protocol {
            core::OutputProtocolKind::Rtp => "rtp",
            core::OutputProtocolKind::Rtmp => "rtmp",
            core::OutputProtocolKind::Mp4 => "mp4",
            core::OutputProtocolKind::Whip => "whip",
            core::OutputProtocolKind::Whep => "whep",
            core::OutputProtocolKind::Hls => "hls",
            core::OutputProtocolKind::MoqClient => "moq_client",
            core::OutputProtocolKind::EncodedDataChannel => "encoded_data",
            core::OutputProtocolKind::RawDataChannel => "raw_data",
        };
        Self {
            output_id,
            output_type: output_type.to_string(),
        }
    }
}
