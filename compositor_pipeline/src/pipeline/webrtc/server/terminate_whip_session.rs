use crate::pipeline::webrtc::{error::WhipServerError, WhipWhepServerState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;
use tracing::info;

pub async fn handle_terminate_whip_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
) -> Result<StatusCode, WhipServerError> {
    let session_id = Arc::from(id);

    state.inputs.validate_token(&session_id, &headers).await?;

    let peer_connection = state
        .inputs
        .get_mut_with(&session_id, |input| Ok(input.peer_connection.take()))?;

    match peer_connection {
        Some(peer_connection) => peer_connection.close().await?,
        None => {
            return Err(WhipServerError::InternalError(format!(
                "None peer connection for {session_id:?}"
            )));
        }
    }

    info!("WHIP session {session_id:?} terminated");
    Ok(StatusCode::OK)
}
