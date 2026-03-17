use std::{sync::Arc, time::Duration};

use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use smelter_api::TypeError;
use utoipa::ToSchema;

use crate::{
    error::ApiError,
    state::{ApiState, Response},
};

use smelter_api::InputId;

use super::Json;

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateInputRequest {
    pub pause: Option<bool>,
    /// Seek to a specific position in milliseconds. Only supported for MP4 inputs.
    pub seek_ms: Option<f64>,
}

#[utoipa::path(
    post,
    path = "/api/input/{input_id}/update",
    operation_id = "update_input",
    params(("input_id" = str, Path, description = "Input ID.")),
    responses(
        (status = 200, description = "Input updated successfully.", body = Response),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["update_request"],
)]
pub async fn handle_input_update(
    State(api): State<Arc<ApiState>>,
    Path(input_id): Path<InputId>,
    Json(request): Json<UpdateInputRequest>,
) -> Result<Response, ApiError> {
    let seek = request
        .seek_ms
        .map(|ms| Duration::try_from_secs_f64(ms / 1000.0))
        .transpose()
        .map_err(|err| TypeError::new(format!("Invalid seek duration. {err}")))?;

    api.pipeline()?
        .lock()
        .unwrap()
        .update_input(&input_id.into(), request.pause, seek)?;
    Ok(Response::Ok {})
}
