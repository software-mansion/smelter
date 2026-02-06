use std::sync::Arc;

use axum::extract::State;
use smelter_core::Pipeline;

use crate::{
    error::ApiError,
    state::{ApiState, Response},
};

pub async fn handle_start(State(state): State<Arc<ApiState>>) -> Result<Response, ApiError> {
    Pipeline::start(&state.pipeline()?);
    Ok(Response::Ok {})
}

pub async fn handle_reset(State(state): State<Arc<ApiState>>) -> Result<Response, ApiError> {
    tokio::task::spawn_blocking(move || state.reset())
        .await
        .unwrap()?;
    Ok(Response::Ok {})
}
