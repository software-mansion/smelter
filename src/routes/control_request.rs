use std::sync::Arc;

use axum::extract::State;
use smelter_api::OkResponse;
use smelter_core::Pipeline;

use crate::{error::ApiError, routes::Json, state::ApiState};

#[utoipa::path(
    post,
    path = "/api/start",
    operation_id = "start",
    responses(
        (status = 200, description = "Smelter instance started.", body = OkResponse),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["control_request"],
)]
pub async fn handle_start(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<OkResponse>, ApiError> {
    Pipeline::start(&state.pipeline()?);
    Ok(Json(OkResponse {}))
}

#[utoipa::path(
    post,
    path = "/api/reset",
    operation_id = "reset",
    responses(
        (status = 200, description = "Smelter instance reset.", body = OkResponse),
        (status = 409, description = "Reset is already in progress.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["control_request"],
)]
pub async fn handle_reset(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<OkResponse>, ApiError> {
    tokio::task::spawn_blocking(move || state.reset())
        .await
        .unwrap()?;
    Ok(Json(OkResponse {}))
}
