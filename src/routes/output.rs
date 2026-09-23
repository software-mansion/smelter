use std::sync::Arc;

use axum::extract::{Path, State};
use smelter_api::{
    OkResponse, OutputId, RegisterOutput, RegisterOutputResponse, UnregisterRequest,
    UpdateOutputRequest,
};
use smelter_core::Pipeline;

use crate::{error::ApiError, routes::Json, state::ApiState};

#[utoipa::path(
    post,
    path = "/api/output/{output_id}/register",
    operation_id = "register_output",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Output registered successfully.", body = RegisterOutputResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_register(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<RegisterOutput>,
) -> Result<Json<RegisterOutputResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let port =
            Pipeline::register_output(&api.pipeline()?, output_id.into(), request.try_into()?)?;
        Ok(Json(port.into()))
    })
    .await
    .unwrap()
}

#[utoipa::path(
    post,
    path = "/api/output/{output_id}/unregister",
    operation_id = "unregister_output",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Output unregistered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Output not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_unregister(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<UnregisterRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    api.schedule_or_run(request.schedule_time()?, move |pipeline| {
        pipeline.unregister_output(&output_id.into())
    })?;
    Ok(Json(OkResponse {}))
}

#[utoipa::path(
    post,
    path = "/api/output/{output_id}/update",
    operation_id = "update_output",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Output updated successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["update_request"],
)]
pub async fn handle_update(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
    Json(request): Json<UpdateOutputRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    let schedule_time = request.schedule_time()?;
    let scene = match request.video {
        Some(component) => Some(component.try_into()?),
        None => None,
    };
    let audio = request.audio.map(|a| a.try_into()).transpose()?;

    api.schedule_or_run(schedule_time, move |pipeline| {
        pipeline.update_output(output_id.into(), scene, audio)
    })?;
    Ok(Json(OkResponse {}))
}

#[utoipa::path(
    post,
    path = "/api/output/{output_id}/request_keyframe",
    operation_id = "request_keyframe",
    params(("output_id" = str, Path, description = "Output ID.")),
    responses(
        (status = 200, description = "Keyframe request successful.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["update_request"],
)]
pub async fn handle_request_keyframe(
    State(api): State<Arc<ApiState>>,
    Path(output_id): Path<OutputId>,
) -> Result<Json<OkResponse>, ApiError> {
    api.pipeline()?
        .lock()
        .unwrap()
        .request_keyframe(output_id.into())?;

    Ok(Json(OkResponse {}))
}
