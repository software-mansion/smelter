use crate::pipeline::webrtc::{error::WhipWhepServerError, WhipWhepServerState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use compositor_render::OutputId;
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whep_session(
    Path((id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipWhepServerError> {
    let output_id = OutputId(Arc::from(id));
    let session_id = Arc::from(session_id);

    state.outputs.validate_token(&output_id, &headers).await?;

    let peer_connection = state.outputs.get_session(&output_id, &session_id)?;

    peer_connection.close().await?;

    info!("WHEP session terminated for output: {:?}", output_id);
    Ok(StatusCode::OK)
}
