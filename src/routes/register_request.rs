use std::sync::Arc;

use axum::extract::{Path, State};
use compositor_pipeline::pipeline::{input::InputInitInfo, Port};
use glyphon::fontdb::Source;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    routes::{Json, Multipart},
    state::{Pipeline, Response},
};
use compositor_api::{
    error::ApiError,
    types::{
        DeckLink, ImageSpec, InputId, Mp4Input, Mp4Output, OutputId, RendererId, RtmpOutput,
        RtpInput, RtpOutput, ShaderSpec, WebRendererSpec, WhipInput, WhipOutput,
    },
};

use super::ApiState;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisterInput {
    RtpStream(RtpInput),
    Mp4(Mp4Input),
    Whip(WhipInput),
    #[serde(rename = "decklink")]
    DeckLink(DeckLink),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisterOutput {
    RtpStream(RtpOutput),
    RtmpStream(RtmpOutput),
    Mp4(Mp4Output),
    Whip(WhipOutput),
}

pub(super) async fn handle_input(
    State(api): State<ApiState>,
    Path(input_id): Path<InputId>,
    Json(request): Json<RegisterInput>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        let response = match request {
            RegisterInput::RtpStream(rtp) => {
                Pipeline::register_input(&api.pipeline, input_id.into(), rtp.try_into()?)?
            }
            RegisterInput::Mp4(mp4) => {
                Pipeline::register_input(&api.pipeline, input_id.into(), mp4.try_into()?)?
            }
            RegisterInput::DeckLink(decklink) => {
                Pipeline::register_input(&api.pipeline, input_id.into(), decklink.try_into()?)?
            }
            RegisterInput::Whip(whip) => {
                Pipeline::register_input(&api.pipeline, input_id.into(), whip.try_into()?)?
            }
        };
        match response {
            InputInitInfo::Rtp { port } => Ok(Response::RegisteredPort {
                port: port.map(|p| p.0),
            }),
            InputInitInfo::Mp4 {
                video_duration,
                audio_duration,
            } => Ok(Response::RegisteredMp4 {
                video_duration_ms: video_duration.map(|v| v.as_millis() as u64),
                audio_duration_ms: audio_duration.map(|a| a.as_millis() as u64),
            }),
            InputInitInfo::Whip { bearer_token } => Ok(Response::BearerToken { bearer_token }),
            InputInitInfo::Other => Ok(Response::Ok {}),
        }
    })
    .await
    // `unwrap()` panics only when the task panicked or `response.abort()` was called
    .unwrap()
}

pub(super) async fn handle_output(
    State(api): State<ApiState>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<RegisterOutput>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        let response = match request {
            RegisterOutput::RtpStream(rtp) => {
                Pipeline::register_output(&api.pipeline, output_id.into(), rtp.try_into()?)?
            }
            RegisterOutput::Mp4(mp4) => {
                Pipeline::register_output(&api.pipeline, output_id.into(), mp4.try_into()?)?
            }
            RegisterOutput::Whip(whip) => {
                Pipeline::register_output(&api.pipeline, output_id.into(), whip.try_into()?)?
            }
            RegisterOutput::RtmpStream(rtmp) => {
                Pipeline::register_output(&api.pipeline, output_id.into(), rtmp.try_into()?)?
            }
        };
        match response {
            Some(Port(port)) => Ok(Response::RegisteredPort { port: Some(port) }),
            None => Ok(Response::Ok {}),
        }
    })
    .await
    .unwrap()
}

pub(super) async fn handle_shader(
    State(api): State<ApiState>,
    Path(shader_id): Path<RendererId>,
    Json(request): Json<ShaderSpec>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline, shader_id.into(), request.try_into()?)?;
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}

pub(super) async fn handle_web_renderer(
    State(api): State<ApiState>,
    Path(instance_id): Path<RendererId>,
    Json(request): Json<WebRendererSpec>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline, instance_id.into(), request.try_into()?)?;
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}

pub(super) async fn handle_image(
    State(api): State<ApiState>,
    Path(image_id): Path<RendererId>,
    Json(request): Json<ImageSpec>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline, image_id.into(), request.try_into()?)?;
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}

pub(super) async fn handle_font(
    State(api): State<ApiState>,
    Multipart(mut multipart): Multipart,
) -> Result<Response, ApiError> {
    let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::malformed_request(&err))?
    else {
        return Err(ApiError::malformed_request(&"Missing font file"));
    };

    let bytes = field
        .bytes()
        .await
        .map_err(|err| ApiError::malformed_request(&err))?;

    let binary_font_source = Source::Binary(Arc::new(bytes));

    tokio::task::spawn_blocking(move || {
        Pipeline::register_font(&api.pipeline.lock().unwrap(), binary_font_source);
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}
