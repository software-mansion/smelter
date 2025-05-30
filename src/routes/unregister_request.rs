use std::time::Duration;

use axum::extract::{Path, State};
use compositor_render::{error::ErrorStack, RegistryType};
use log::error;
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiError,
    state::{ApiState, Response},
};

use compositor_api::{InputId, OutputId, RendererId};

use super::Json;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnregisterInput {
    /// Time in milliseconds when this request should be applied. Value `0` represents
    /// time of the start request.
    schedule_time_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnregisterOutput {
    /// Time in milliseconds when this request should be applied. Value `0` represents
    /// time of the start request.
    schedule_time_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnregisterRenderer {
    /// Time in milliseconds when this request should be applied. Value `0` represents
    /// time of the start request.
    schedule_time_ms: Option<f64>,
}

pub(super) async fn handle_input(
    State(api): State<ApiState>,
    Path(input_id): Path<InputId>,
    Json(request): Json<UnregisterInput>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let pipeline = api.pipeline.clone();
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            api.pipeline().queue().schedule_event(
                schedule_time,
                Box::new(move || {
                    if let Err(err) = pipeline.lock().unwrap().unregister_input(&input_id.into()) {
                        error!(
                            "Error while running scheduled input unregister for pts {}ms: {}",
                            schedule_time.as_millis(),
                            ErrorStack::new(&err).into_string()
                        )
                    }
                }),
            );
        }
        None => {
            api.pipeline().unregister_input(&input_id.into())?;
        }
    }
    Ok(Response::Ok {})
}

pub(super) async fn handle_output(
    State(api): State<ApiState>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<UnregisterOutput>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let pipeline = api.pipeline.clone();
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            api.pipeline().queue().schedule_event(
                schedule_time,
                Box::new(move || {
                    if let Err(err) = pipeline
                        .lock()
                        .unwrap()
                        .unregister_output(&output_id.into())
                    {
                        error!(
                            "Error while running scheduled output unregister for pts {}ms: {}",
                            schedule_time.as_millis(),
                            ErrorStack::new(&err).into_string()
                        )
                    }
                }),
            );
        }
        None => {
            api.pipeline().unregister_output(&output_id.into())?;
        }
    }
    Ok(Response::Ok {})
}

pub(super) async fn handle_shader(
    State(api): State<ApiState>,
    Path(shader_id): Path<RendererId>,
    Json(request): Json<UnregisterRenderer>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let pipeline = api.pipeline.clone();
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            api.pipeline().queue().schedule_event(
                schedule_time,
                Box::new(move || {
                    if let Err(err) = pipeline
                        .lock()
                        .unwrap()
                        .unregister_renderer(&shader_id.into(), RegistryType::Shader)
                    {
                        error!(
                            "Error while running scheduled shader unregister for pts {}ms: {}",
                            schedule_time.as_millis(),
                            ErrorStack::new(&err).into_string()
                        )
                    }
                }),
            );
        }
        None => {
            api.pipeline()
                .unregister_renderer(&shader_id.into(), RegistryType::Shader)?;
        }
    }
    Ok(Response::Ok {})
}

pub(super) async fn handle_web_renderer(
    State(api): State<ApiState>,
    Path(instance_id): Path<RendererId>,
    Json(request): Json<UnregisterRenderer>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let pipeline = api.pipeline.clone();
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            api.pipeline().queue().schedule_event(
                schedule_time,
                Box::new(move || {
                    if let Err(err) = pipeline
                        .lock()
                        .unwrap()
                        .unregister_renderer(&instance_id.into(), RegistryType::WebRenderer)
                    {
                        error!(
                            "Error while running scheduled web renderer unregister for pts {}ms: {}",
                            schedule_time.as_millis(),
                            ErrorStack::new(&err).into_string()
                        )
                    }
                }),
            );
        }
        None => {
            api.pipeline()
                .unregister_renderer(&instance_id.into(), RegistryType::WebRenderer)?;
        }
    }
    Ok(Response::Ok {})
}

pub(super) async fn handle_image(
    State(api): State<ApiState>,
    Path(image_id): Path<RendererId>,
    Json(request): Json<UnregisterRenderer>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let pipeline = api.pipeline.clone();
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            api.pipeline().queue().schedule_event(
                schedule_time,
                Box::new(move || {
                    if let Err(err) = pipeline
                        .lock()
                        .unwrap()
                        .unregister_renderer(&image_id.into(), RegistryType::Image)
                    {
                        error!(
                            "Error while running scheduled image unregister for pts {}ms: {}",
                            schedule_time.as_millis(),
                            ErrorStack::new(&err).into_string()
                        )
                    }
                }),
            );
        }
        None => {
            api.pipeline()
                .unregister_renderer(&image_id.into(), RegistryType::Image)?;
        }
    }
    Ok(Response::Ok {})
}
