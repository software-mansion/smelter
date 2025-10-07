use crate::pipeline::webrtc::{
    WhipWhepServerState,
    error::WhipWhepServerError,
    trickle_ice_utils::{ice_fragment_unmarshal, validate_content_type},
};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;
use tracing::info;

pub async fn handle_new_whip_ice_candidates(
    Path((endpoint_id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    sdp_fragment_content: String,
) -> Result<StatusCode, WhipWhepServerError> {
    let endpoint_id = Arc::from(endpoint_id);
    let session_id = Arc::from(session_id);

    validate_content_type(&headers)?;
    state.inputs.validate_token(&endpoint_id, &headers).await?;
    state
        .inputs
        .validate_session_id(&endpoint_id, &session_id)?;

    let peer_connection = state
        .inputs
        .get_with(&endpoint_id, |input| Ok(input.peer_connection.clone()))?;

    if let Some(peer_connection) = peer_connection {
        for candidate in ice_fragment_unmarshal(&sdp_fragment_content) {
            if let Err(err) = peer_connection.add_ice_candidate(candidate.clone()).await {
                return Err(WhipWhepServerError::BadRequest(format!(
                    "Cannot add ice_candidate {candidate:?} for session {session_id:?} (endpoint {endpoint_id:?}): {err:?}"
                )));
            }
            info!(
                ?session_id,
                ?endpoint_id,
                "Added ICE candidate for WHIP session"
            );
        }
    } else {
        return Err(WhipWhepServerError::InternalError(format!(
            "None peer connection for {endpoint_id:?}"
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}
