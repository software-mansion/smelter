use crate::pipeline::webrtc::{error::WhipWhepServerError, WhipWhepServerState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whip_session(
    Path((id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipWhepServerError> {
    let input_id = Arc::from(id);
    let session_id = Arc::from(session_id);

    state.inputs.validate_token(&input_id, &headers).await?;

    let peer_connection = state.inputs.get_session(&input_id, &session_id)?;

    peer_connection.close().await?;

    info!(?session_id, ?input_id, "WHIP sessionterminated");
    Ok(StatusCode::OK)
}
