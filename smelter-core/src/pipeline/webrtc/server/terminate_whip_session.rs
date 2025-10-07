use crate::pipeline::webrtc::{WhipWhepServerState, error::WhipWhepServerError};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whip_session(
    Path((endpoint_id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipWhepServerError> {
    let endpoint_id = Arc::from(endpoint_id);
    let session_id = Arc::from(session_id);

    state.inputs.validate_token(&endpoint_id, &headers).await?;
    state
        .inputs
        .validate_session_id(&endpoint_id, &session_id)?;

    let peer_connection = state
        .inputs
        .get_mut_with(&endpoint_id, |input| Ok(input.peer_connection.take()))?;

    match peer_connection {
        Some(peer_connection) => peer_connection.close().await?,
        None => {
            return Err(WhipWhepServerError::InternalError(format!(
                "None peer connection for {session_id:?}"
            )));
        }
    }

    info!("WHIP session {session_id:?} terminated");
    Ok(StatusCode::OK)
}
