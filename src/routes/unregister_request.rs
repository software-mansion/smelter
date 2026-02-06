use std::{sync::Arc, time::Duration};

use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use smelter_core::Pipeline;
use smelter_render::{RegistryType, error::ErrorStack};
use tracing::error;
use utoipa::ToSchema;

use crate::{
    error::ApiError,
    state::{ApiState, Response},
};

use smelter_api::{InputId, OutputId, RendererId};

use super::Json;

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UnregisterInput {
    /// Time in milliseconds when this request should be applied. Value `0` represents
    /// time of the start request.
    schedule_time_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UnregisterOutput {
    /// Time in milliseconds when this request should be applied. Value `0` represents
    /// time of the start request.
    schedule_time_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct UnregisterRenderer {
    /// Time in milliseconds when this request should be applied. Value `0` represents
    /// time of the start request.
    schedule_time_ms: Option<f64>,
}

#[utoipa::path(
    post,
    path = "/api/input/{input_id}/unregister",
    operation_id = "unregister_input",
    params(("input_id" = str, Path, description = "Input ID.")),
    responses(
        (status = 200, description = "Input unregistered successfully.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Input not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_input(
    State(api): State<Arc<ApiState>>,
    Path(input_id): Path<InputId>,
    Json(request): Json<UnregisterInput>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            Pipeline::schedule_event(&api.pipeline()?, schedule_time, move |pipeline| {
                if let Err(err) = pipeline.unregister_input(&input_id.into()) {
                    error!(
                        "Error while running scheduled input unregister for pts {}ms: {}",
                        schedule_time.as_millis(),
                        ErrorStack::new(&err).into_string()
                    )
                }
            });
        }
        None => {
            api.pipeline()?
                .lock()
                .unwrap()
                .unregister_input(&input_id.into())?;
        }
    }
    Ok(Response::Ok {})
}

#[utoipa::path(
    post,
    path = "/api/input/{output_id}/unregister",
    operation_id = "unregister_output",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Output unregistered successfully.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Output not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_output(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<UnregisterOutput>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            Pipeline::schedule_event(&api.pipeline()?, schedule_time, move |pipeline| {
                if let Err(err) = pipeline.unregister_output(&output_id.into()) {
                    error!(
                        "Error while running scheduled output unregister for pts {}ms: {}",
                        schedule_time.as_millis(),
                        ErrorStack::new(&err).into_string()
                    )
                }
            });
        }
        None => {
            api.pipeline()?
                .lock()
                .unwrap()
                .unregister_output(&output_id.into())?;
        }
    }
    Ok(Response::Ok {})
}

#[utoipa::path(
    post,
    path = "/api/input/{shader_id}/unregister",
    operation_id = "unregister_shader",
    params(("shader_id" = str, Path, description = "Shader ID.")),
    responses(
        (status = 200, description = "Shader unregistered successfully.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Shader not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_shader(
    State(api): State<Arc<ApiState>>,
    Path(shader_id): Path<RendererId>,
    Json(request): Json<UnregisterRenderer>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            Pipeline::schedule_event(&api.pipeline()?, schedule_time, move |pipeline| {
                if let Err(err) =
                    pipeline.unregister_renderer(&shader_id.into(), RegistryType::Shader)
                {
                    error!(
                        "Error while running scheduled shader unregister for pts {}ms: {}",
                        schedule_time.as_millis(),
                        ErrorStack::new(&err).into_string()
                    )
                }
            });
        }
        None => {
            api.pipeline()?
                .lock()
                .unwrap()
                .unregister_renderer(&shader_id.into(), RegistryType::Shader)?;
        }
    }
    Ok(Response::Ok {})
}

#[utoipa::path(
    post,
    path = "/api/web-renderer/{instance_id}/unregister",
    operation_id = "unregister_web_renderer",
    params(("instance_id" = str, Path, description = "Web renderer ID.")),
    responses(
        (status = 200, description = "Web renderer unregistered successfully.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Web renderer not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_web_renderer(
    State(api): State<Arc<ApiState>>,
    Path(instance_id): Path<RendererId>,
    Json(request): Json<UnregisterRenderer>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            Pipeline::schedule_event(&api.pipeline()?, schedule_time, move |pipeline| {
                if let Err(err) =
                    pipeline.unregister_renderer(&instance_id.into(), RegistryType::WebRenderer)
                {
                    error!(
                        "Error while running scheduled web renderer unregister for pts {}ms: {}",
                        schedule_time.as_millis(),
                        ErrorStack::new(&err).into_string()
                    )
                }
            });
        }
        None => {
            api.pipeline()?
                .lock()
                .unwrap()
                .unregister_renderer(&instance_id.into(), RegistryType::WebRenderer)?;
        }
    }
    Ok(Response::Ok {})
}

#[utoipa::path(
    post,
    path = "/api/image/{image_id}/unregister",
    operation_id = "unregister_image",
    params(("image_id" = str, Path, description = "Image ID.")),
    responses(
        (status = 200, description = "Image unregistered successfully.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Image not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_image(
    State(api): State<Arc<ApiState>>,
    Path(image_id): Path<RendererId>,
    Json(request): Json<UnregisterRenderer>,
) -> Result<Response, ApiError> {
    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            Pipeline::schedule_event(&api.pipeline()?, schedule_time, move |pipeline| {
                if let Err(err) =
                    pipeline.unregister_renderer(&image_id.into(), RegistryType::Image)
                {
                    error!(
                        "Error while running scheduled image unregister for pts {}ms: {}",
                        schedule_time.as_millis(),
                        ErrorStack::new(&err).into_string()
                    )
                }
            });
        }
        None => {
            api.pipeline()?
                .lock()
                .unwrap()
                .unregister_renderer(&image_id.into(), RegistryType::Image)?;
        }
    }
    Ok(Response::Ok {})
}
