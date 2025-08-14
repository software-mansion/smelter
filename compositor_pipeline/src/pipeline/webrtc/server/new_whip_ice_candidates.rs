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
use tracing::info;

pub async fn handle_new_whip_ice_candidates(
    Path((id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    sdp_fragment_content: String,
) -> Result<StatusCode, WhipWhepServerError> {
    let input_id = Arc::from(id);
    let session_id = Arc::from(session_id);

    validate_content_type(&headers)?;
    state.inputs.validate_token(&input_id, &headers).await?;

    let peer_connection = state.inputs.get_session(&input_id, &session_id)?;

    for candidate in ice_fragment_unmarshal(&sdp_fragment_content) {
        if let Err(err) = peer_connection.add_ice_candidate(candidate.clone()).await {
            return Err(WhipWhepServerError::BadRequest(format!(
                "Cannot add ice_candidate {candidate:?} for session {input_id:?}: {err:?}"
            )));
        }
        info!(
            ?session_id,
            ?input_id,
            "Added ICE candidate for WHIP session"
        );
    }

    Ok(StatusCode::NO_CONTENT)
}
