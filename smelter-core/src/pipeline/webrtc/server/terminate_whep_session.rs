use crate::pipeline::webrtc::{WhipWhepServerState, error::WhipWhepServerError};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whep_session(
    Path((output_id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipWhepServerError> {
    let output_ref = state.outputs.resolve_output_ref(&output_id)?;
    let session_id = Arc::from(session_id);

    state.outputs.validate_token(&output_ref, &headers)?;
    state.outputs.remove_session(&output_ref, &session_id)?;

    info!(?session_id, output_id, "WHEP session terminated");
    Ok(StatusCode::OK)
}
