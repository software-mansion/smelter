use crate::pipeline::webrtc::{
    error::WhipWhepServerError,
    trickle_ice_utils::{ice_fragment_unmarshal, validate_content_type},
    WhipWhepServerState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use compositor_render::OutputId;

use std::sync::Arc;

pub async fn handle_new_whep_ice_candidates(
    Path((id, session_id)): Path<(String, String)>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    sdp_fragment_content: String,
) -> Result<StatusCode, WhipWhepServerError> {
    let output_id = OutputId(Arc::from(id));
    let session_id = Arc::from(session_id);

    validate_content_type(&headers)?;
    state.outputs.validate_token(&output_id, &headers).await?;

    let peer_connection = state.outputs.get_session(&output_id, &session_id)?;

    for candidate in ice_fragment_unmarshal(&sdp_fragment_content) {
        if let Err(err) = peer_connection.add_ice_candidate(candidate.clone()).await {
            return Err(WhipWhepServerError::BadRequest(format!(
                "Cannot add ice_candidate {candidate:?} for output {output_id:?} session {session_id:?}: {err:?}"
            )));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}
