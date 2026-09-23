use std::sync::Arc;

use axum::extract::{Path, State};
use smelter_api::{
    InputId, OkResponse, RegisterInput, RegisterInputResponse, UnregisterRequest,
    UpdateInputRequest,
};
use smelter_core::Pipeline;

use crate::{error::ApiError, routes::Json, state::ApiState};

#[utoipa::path(
    post,
    path = "/api/input/{input_id}/register",
    operation_id = "register_input",
    params(("input_id" = str, Path, description = "Input ID.")),
    responses(
        (status = 200, description = "Input registered successfully.", body = RegisterInputResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 422, description = "Invalid request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["register_request"],
)]
pub async fn handle_register(
    State(api): State<Arc<ApiState>>,
    Path(input_id): Path<InputId>,
    Json(request): Json<RegisterInput>,
) -> Result<Json<RegisterInputResponse>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let info =
            Pipeline::register_input(&api.pipeline()?, input_id.into(), request.try_into()?)?;
        Ok(Json(info.into()))
    })
    .await
    // `unwrap()` panics only when the task panicked or `response.abort()` was called
    .unwrap()
}

#[utoipa::path(
    post,
    path = "/api/input/{input_id}/unregister",
    operation_id = "unregister_input",
    params(("input_id" = str, Path, description = "Input ID.")),
    responses(
        (status = 200, description = "Input unregistered successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 422, description = "Invalid request.", body = ApiError),
        (status = 404, description = "Input not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["unregister_request"],
)]
pub async fn handle_unregister(
    State(api): State<Arc<ApiState>>,
    Path(input_id): Path<InputId>,
    Json(request): Json<UnregisterRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    api.schedule_or_run(request.schedule_time()?, move |pipeline| {
        pipeline.unregister_input(&input_id.into())
    })?;
    Ok(Json(OkResponse {}))
}

#[utoipa::path(
    post,
    path = "/api/input/{input_id}/update",
    operation_id = "update_input",
    params(("input_id" = str, Path, description = "Input ID.")),
    responses(
        (status = 200, description = "Input updated successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 422, description = "Invalid request.", body = ApiError),
        (status = 404, description = "Input not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["update_request"],
)]
pub async fn handle_update(
    State(api): State<Arc<ApiState>>,
    Path(input_id): Path<InputId>,
    Json(request): Json<UpdateInputRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    let seek = request.seek()?;

    api.pipeline()?
        .lock()
        .unwrap()
        .update_input(&input_id.into(), request.pause, seek)?;
    Ok(Json(OkResponse {}))
}
