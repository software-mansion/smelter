use std::{sync::Arc, time::Duration};

use axum::extract::{Path, State};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smelter_core::Pipeline;
use smelter_render::error::ErrorStack;
use tracing::error;
use utoipa::ToSchema;

use crate::{
    error::ApiError,
    state::{ApiState, Response},
};

use smelter_api::{AudioScene, OutputId, VideoScene};

use super::Json;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateOutputRequest {
    pub video: Option<VideoScene>,
    pub audio: Option<AudioScene>,
    pub schedule_time_ms: Option<f64>,
}

#[utoipa::path(
    post,
    path = "/api/output/{output_id}/update",
    operation_id = "update_output",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Output updated successfully.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["update_request"],
)]
pub async fn handle_output_update(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<UpdateOutputRequest>,
) -> Result<Response, ApiError> {
    let output_id = output_id.into();
    let scene = match request.video {
        Some(component) => Some(component.try_into()?),
        None => None,
    };
    let audio = request.audio.map(|a| a.try_into()).transpose()?;

    match request.schedule_time_ms {
        Some(schedule_time_ms) => {
            let schedule_time = Duration::from_secs_f64(schedule_time_ms / 1000.0);
            Pipeline::schedule_event(&api.pipeline()?, schedule_time, move |pipeline| {
                if let Err(err) = pipeline.update_output(output_id, scene, audio) {
                    error!(
                        "Error while running scheduled output update for pts {}ms: {}",
                        schedule_time.as_millis(),
                        ErrorStack::new(&err).into_string()
                    )
                }
            });
        }
        None => api
            .pipeline()?
            .lock()
            .unwrap()
            .update_output(output_id, scene, audio)?,
    };
    Ok(Response::Ok {})
}

#[utoipa::path(
    post,
    path = "/api/output/{output_id}/request_keyframe",
    operation_id = "request_keyframe",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Keyframe request successful.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["update_request"],
)]
pub async fn handle_keyframe_request(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
) -> Result<Response, ApiError> {
    api.pipeline()?
        .lock()
        .unwrap()
        .request_keyframe(output_id.into())?;

    Ok(Response::Ok {})
}
