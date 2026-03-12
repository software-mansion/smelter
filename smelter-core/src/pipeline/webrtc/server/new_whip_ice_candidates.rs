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
    let input_ref = state.inputs.find_by_endpoint_id(&Arc::from(endpoint_id))?;
    let session_id = Arc::from(session_id);

    validate_content_type(&headers)?;
    state.inputs.validate_token(&input_ref, &headers).await?;
    state.inputs.validate_session_id(&input_ref, &session_id)?;

    let session = state
        .inputs
        .get_with(&input_ref, |input| match &input.session {
            Some(s) => Ok(Some((s.peer_connection.downgrade(), s.session_id.clone()))),
            None => Ok(None),
        })?;

    let Some((peer_connection, session_id)) = session else {
        return Err(WhipWhepServerError::InternalError(format!(
            "None peer connection for {input_ref}"
        )));
    };

    let Some(peer_connection) = peer_connection.upgrade() else {
        return Err(WhipWhepServerError::BadRequest(
            "Peer connection already closed".to_string(),
        ));
    };

    for candidate in ice_fragment_unmarshal(&sdp_fragment_content) {
        if let Err(err) = peer_connection.add_ice_candidate(candidate.clone()).await {
            return Err(WhipWhepServerError::BadRequest(format!(
                "Cannot add ice_candidate {candidate:?} for session {session_id:?} (input_id {input_ref}): {err:?}"
            )));
        }
        info!(
            ?session_id,
            input_id=%input_ref,
            "Added ICE candidate for WHIP session"
        );
    }

    Ok(StatusCode::NO_CONTENT)
}
