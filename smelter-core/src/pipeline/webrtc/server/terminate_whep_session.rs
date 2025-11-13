use crate::pipeline::webrtc::{WhipWhepServerState, error::WhipWhepServerError};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whep_session(
    Path((endpoint_id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipWhepServerError> {
    let endpoint_id = Arc::from(endpoint_id.clone());
    let output_ref = state.outputs.find_by_endpoint_id(&endpoint_id)?;
    let session_id = Arc::from(session_id);

    state.outputs.validate_token(&output_ref, &headers).await?;

    state
        .outputs
        .remove_session(&output_ref, &session_id)
        .await?;

    info!(?session_id, output_id=?output_ref.id(), "WHEP session terminated");
    Ok(StatusCode::OK)
}
