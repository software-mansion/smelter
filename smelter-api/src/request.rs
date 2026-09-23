use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::common_core::prelude as core;
use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisterInput {
    RtpStream(RtpInput),
    RtmpServer(RtmpInput),
    MoqServer(MoqServerInput),
    MoqClient(MoqClientInput),
    Mp4(Mp4Input),
    WhipServer(WhipInput),
    WhepClient(WhepInput),
    Hls(HlsInput),
    V4l2(V4l2Input),
    #[serde(rename = "decklink")]
    DeckLink(DeckLink),
}

impl TryFrom<RegisterInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: RegisterInput) -> Result<Self, Self::Error> {
        match value {
            RegisterInput::RtpStream(rtp) => rtp.try_into(),
            RegisterInput::RtmpServer(rtmp) => rtmp.try_into(),
            RegisterInput::MoqServer(moq_server) => moq_server.try_into(),
            RegisterInput::MoqClient(moq_client) => moq_client.try_into(),
            RegisterInput::Mp4(mp4) => mp4.try_into(),
            RegisterInput::WhipServer(whip) => whip.try_into(),
            RegisterInput::WhepClient(whep) => whep.try_into(),
            RegisterInput::Hls(hls) => hls.try_into(),
            RegisterInput::V4l2(v4l2) => v4l2.try_into(),
            RegisterInput::DeckLink(decklink) => decklink.try_into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisterOutput {
    RtpStream(RtpOutput),
    RtmpClient(RtmpOutput),
    MoqClient(MoqClientOutput),
    Mp4(Mp4Output),
    WhipClient(WhipOutput),
    WhepServer(WhepOutput),
    Hls(HlsOutput),
}

impl TryFrom<RegisterOutput> for core::RegisterOutputOptions {
    type Error = TypeError;

    fn try_from(value: RegisterOutput) -> Result<Self, Self::Error> {
        match value {
            RegisterOutput::RtpStream(rtp) => rtp.try_into(),
            RegisterOutput::RtmpClient(rtmp) => rtmp.try_into(),
            RegisterOutput::MoqClient(moq_client) => moq_client.try_into(),
            RegisterOutput::Mp4(mp4) => mp4.try_into(),
            RegisterOutput::WhipClient(whip) => whip.try_into(),
            RegisterOutput::WhepServer(whep) => whep.try_into(),
            RegisterOutput::Hls(hls) => hls.try_into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateInputRequest {
    pub pause: Option<bool>,
    /// Seek to a specific position in milliseconds. Only supported for MP4 inputs.
    pub seek_ms: Option<f64>,
}

impl UpdateInputRequest {
    pub fn seek(&self) -> Result<Option<Duration>, TypeError> {
        self.seek_ms
            .map(|ms| Duration::try_from_secs_f64(ms / 1000.0))
            .transpose()
            .map_err(|err| TypeError::new(format!("Invalid seek duration. {err}")))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateOutputRequest {
    pub video: Option<VideoScene>,
    pub audio: Option<AudioScene>,
    pub schedule_time_ms: Option<f64>,
}

/// Request body shared by all unregister routes.
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UnregisterRequest {
    /// Time in milliseconds when this request should be applied. Value `0` represents
    /// time of the start request.
    pub schedule_time_ms: Option<f64>,
}
