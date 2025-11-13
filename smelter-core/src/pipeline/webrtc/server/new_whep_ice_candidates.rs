use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use tracing::info;

use crate::pipeline::webrtc::{
    WhipWhepServerState,
    error::WhipWhepServerError,
    trickle_ice_utils::{ice_fragment_unmarshal, validate_content_type},
};

pub async fn handle_new_whep_ice_candidates(
    Path((endpoint_id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    sdp_fragment_content: String,
) -> Result<StatusCode, WhipWhepServerError> {
    let endpoint_id = Arc::from(endpoint_id.clone());
    let output_ref = state.outputs.find_by_endpoint_id(&endpoint_id)?;
    let session_id = Arc::from(session_id);

    validate_content_type(&headers)?;
    state.outputs.validate_token(&output_ref, &headers).await?;

    let peer_connection = state.outputs.get_session(&output_ref, &session_id)?;

    for candidate in ice_fragment_unmarshal(&sdp_fragment_content) {
        if let Err(err) = peer_connection.add_ice_candidate(candidate.clone()).await {
            return Err(WhipWhepServerError::BadRequest(format!(
                "Cannot add ice_candidate {candidate:?} for output {output_ref} session {session_id:?}: {err:?}"
            )));
        }
        info!(
            ?session_id,
            output_id=?output_ref.id(),
            "Added ICE candidate for WHEP session"
        );
    }

    Ok(StatusCode::NO_CONTENT)
}
