use crate::pipeline::webrtc::{
    error::WhipWhepServerError,
    trickle_ice_utils::{ice_fragment_unmarshal, validate_content_type},
    WhipWhepServerState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;

pub async fn handle_new_whip_ice_candidates(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    sdp_fragment_content: String,
) -> Result<StatusCode, WhipWhepServerError> {
    let session_id = Arc::from(id);

    validate_content_type(&headers)?;
    state.inputs.validate_token(&session_id, &headers).await?;

    let peer_connection = state
        .inputs
        .get_with(&session_id, |input| Ok(input.peer_connection.clone()))?;

    if let Some(peer_connection) = peer_connection {
        for candidate in ice_fragment_unmarshal(&sdp_fragment_content) {
            if let Err(err) = peer_connection.add_ice_candidate(candidate.clone()).await {
                return Err(WhipWhepServerError::BadRequest(format!(
                    "Cannot add ice_candidate {candidate:?} for session {session_id:?}: {err:?}"
                )));
            }
        }
    } else {
        return Err(WhipWhepServerError::InternalError(format!(
            "None peer connection for {session_id:?}"
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}
