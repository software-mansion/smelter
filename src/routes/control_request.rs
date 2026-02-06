use std::sync::Arc;

use axum::extract::State;
use smelter_core::Pipeline;

use crate::{
    error::ApiError,
    state::{ApiState, Response},
};

#[utoipa::path(
    post,
    path = "/api/start",
    operation_id = "start",
    responses(
        (status = 200, description = "Smelter instance started.", body = Response),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["control_request"],
)]
pub async fn handle_start(State(state): State<Arc<ApiState>>) -> Result<Response, ApiError> {
    Pipeline::start(&state.pipeline()?);
    Ok(Response::Ok {})
}

#[utoipa::path(
    post,
    path = "/api/reset",
    operation_id = "reset",
    responses(
        (status = 200, description = "Smelter instance reset.", body = Response),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["control_request"],
)]
pub async fn handle_reset(State(state): State<Arc<ApiState>>) -> Result<Response, ApiError> {
    tokio::task::spawn_blocking(move || state.reset())
        .await
        .unwrap()?;
    Ok(Response::Ok {})
}
