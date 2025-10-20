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
    let input_id = state.inputs.find_by_endpoint_id(&Arc::from(endpoint_id))?;
    let session_id = Arc::from(session_id);

    state.inputs.validate_token(&input_id, &headers).await?;
    state.inputs.validate_session_id(&input_id, &session_id)?;

    let session = state
        .inputs
        .get_mut_with(&input_id, |input| Ok(input.session.take()))?;

    match session {
        Some(session) => session.peer_connection.close().await?,
        None => {
            return Err(WhipWhepServerError::InternalError(format!(
                "None peer connection for {session_id:?}"
            )));
        }
    }

    info!("WHIP session {session_id:?} terminated");
    Ok(StatusCode::OK)
}
