use std::sync::Arc;

use axum::extract::{Path, State};
use glyphon::fontdb::Source;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smelter_core::{InputInitInfo, Pipeline, protocols::Port};

use crate::{
    error::ApiError,
    routes::{Json, Multipart},
    state::Response,
};
use smelter_api::{
    DeckLink, HlsInput, HlsOutput, ImageSpec, InputId, Mp4Input, Mp4Output, OutputId, RendererId,
    RtmpOutput, RtpInput, RtpOutput, ShaderSpec, V4l2Input, WebRendererSpec, WhepInput, WhepOutput,
    WhipInput, WhipOutput,
};

use super::ApiState;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisterInput {
    RtpStream(RtpInput),
    Mp4(Mp4Input),
    WhipServer(WhipInput),
    WhepClient(WhepInput),
    Hls(HlsInput),
    V4l2(V4l2Input),
    #[serde(rename = "decklink")]
    DeckLink(DeckLink),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisterOutput {
    RtpStream(RtpOutput),
    RtmpClient(RtmpOutput),
    Mp4(Mp4Output),
    WhipClient(WhipOutput),
    WhepServer(WhepOutput),
    Hls(HlsOutput),
}

pub(super) async fn handle_input(
    State(api): State<Arc<ApiState>>,
    Path(input_id): Path<InputId>,
    Json(request): Json<RegisterInput>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        let response = match request {
            RegisterInput::RtpStream(rtp) => {
                Pipeline::register_input(&api.pipeline()?, input_id.into(), rtp.try_into()?)?
            }
            RegisterInput::Mp4(mp4) => {
                Pipeline::register_input(&api.pipeline()?, input_id.into(), mp4.try_into()?)?
            }
            RegisterInput::DeckLink(decklink) => {
                Pipeline::register_input(&api.pipeline()?, input_id.into(), decklink.try_into()?)?
            }
            RegisterInput::WhipServer(whip) => {
                Pipeline::register_input(&api.pipeline()?, input_id.into(), whip.try_into()?)?
            }
            RegisterInput::WhepClient(whep) => {
                Pipeline::register_input(&api.pipeline()?, input_id.into(), whep.try_into()?)?
            }
            RegisterInput::Hls(hls) => {
                Pipeline::register_input(&api.pipeline()?, input_id.into(), hls.try_into()?)?
            }
            RegisterInput::V4l2(v4l2) => {
                Pipeline::register_input(&api.pipeline()?, input_id.into(), v4l2.try_into()?)?
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
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<RegisterOutput>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        let response = match request {
            RegisterOutput::RtpStream(rtp) => {
                Pipeline::register_output(&api.pipeline()?, output_id.into(), rtp.try_into()?)?
            }
            RegisterOutput::Mp4(mp4) => {
                Pipeline::register_output(&api.pipeline()?, output_id.into(), mp4.try_into()?)?
            }
            RegisterOutput::WhipClient(whip) => {
                Pipeline::register_output(&api.pipeline()?, output_id.into(), whip.try_into()?)?
            }
            RegisterOutput::WhepServer(whep) => {
                Pipeline::register_output(&api.pipeline()?, output_id.into(), whep.try_into()?)?
            }
            RegisterOutput::RtmpClient(rtmp) => {
                Pipeline::register_output(&api.pipeline()?, output_id.into(), rtmp.try_into()?)?
            }
            RegisterOutput::Hls(hls) => {
                Pipeline::register_output(&api.pipeline()?, output_id.into(), hls.try_into()?)?
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
    State(api): State<Arc<ApiState>>,
    Path(shader_id): Path<RendererId>,
    Json(request): Json<ShaderSpec>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline()?, shader_id.into(), request.try_into()?)?;
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}

pub(super) async fn handle_web_renderer(
    State(api): State<Arc<ApiState>>,
    Path(instance_id): Path<RendererId>,
    Json(request): Json<WebRendererSpec>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline()?, instance_id.into(), request.try_into()?)?;
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}

pub(super) async fn handle_image(
    State(api): State<Arc<ApiState>>,
    Path(image_id): Path<RendererId>,
    Json(request): Json<ImageSpec>,
) -> Result<Response, ApiError> {
    let api = api.clone();
    tokio::task::spawn_blocking(move || {
        Pipeline::register_renderer(&api.pipeline()?, image_id.into(), request.try_into()?)?;
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}

pub(super) async fn handle_font(
    State(api): State<Arc<ApiState>>,
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
        api.pipeline()?
            .lock()
            .unwrap()
            .register_font(binary_font_source);
        Ok(Response::Ok {})
    })
    .await
    .unwrap()
}
