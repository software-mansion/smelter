use crate::pipeline::webrtc::{WhipWhepServerState, error::WhipWhepServerError};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use smelter_render::OutputId;
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whep_session(
    Path((endpoint_id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipWhepServerError> {
    let output_id = OutputId(Arc::from(endpoint_id));
    let session_id = Arc::from(session_id);

    state.outputs.validate_token(&output_id, &headers).await?;

    state
        .outputs
        .remove_session(&output_id, &session_id)
        .await?;

    info!(?session_id, ?output_id, "WHEP session terminated");
    Ok(StatusCode::OK)
}
