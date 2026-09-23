use std::sync::Arc;

use axum::extract::{Path, State};

use crate::{error::ApiError, state::ApiState};

use smelter_api::{InputId, OkResponse, UpdateInputRequest};

use super::Json;

#[utoipa::path(
    post,
    path = "/api/input/{input_id}/update",
    operation_id = "update_input",
    params(("input_id" = str, Path, description = "Input ID.")),
    responses(
        (status = 200, description = "Input updated successfully.", body = OkResponse),
        (status = 400, description = "Bad request.", body = ApiError),
        (status = 404, description = "Input not found.", body = ApiError),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["update_request"],
)]
pub async fn handle_input_update(
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
