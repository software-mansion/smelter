use crate::pipeline::webrtc::{error::WhipServerError, WhipWhepServerState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::sync::Arc;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

pub async fn handle_new_whip_ice_candidates(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    sdp_fragment_content: String,
) -> Result<StatusCode, WhipServerError> {
    let session_id = Arc::from(id);

    validate_content_type(&headers)?;
    state.inputs.validate_token(&session_id, &headers).await?;

    let peer_connection = state
        .inputs
        .get_with(&session_id, |input| Ok(input.peer_connection.clone()))?;

    if let Some(peer_connection) = peer_connection {
        for candidate in ice_fragment_unmarshal(&sdp_fragment_content) {
            if let Err(err) = peer_connection.add_ice_candidate(candidate.clone()).await {
                return Err(WhipServerError::BadRequest(format!(
                    "Cannot add ice_candidate {candidate:?} for session {session_id:?}: {err:?}"
                )));
            }
        }
    } else {
        return Err(WhipServerError::InternalError(format!(
            "None peer connection for {session_id:?}"
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub fn ice_fragment_unmarshal(sdp_fragment_content: &str) -> Vec<RTCIceCandidateInit> {
    let lines = sdp_fragment_content.split("\n");
    let mut candidates = Vec::new();
    let mut mid = None;
    let mut mid_num = None;

    for line in lines {
        if line.starts_with("a=mid:") {
            mid = line
                .split_once(':')
                .map(|(_, value)| value.trim().to_string());
            mid_num = mid_num.map_or(Some(0), |index| Some(index + 1));
        }
        if line.starts_with("a=candidate:") {
            candidates.push(RTCIceCandidateInit {
                candidate: line[2..].to_string(),
                sdp_mid: mid.clone(),
                sdp_mline_index: mid_num,
                ..Default::default()
            });
        }
    }
    candidates
}

pub fn validate_content_type(headers: &HeaderMap) -> Result<(), WhipServerError> {
    let content_type = headers
        .get("Content-Type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if content_type != "application/trickle-ice-sdpfrag" {
        return Err(WhipServerError::BadRequest(
            "Invalid Content-Type".to_owned(),
        ));
    }
    Ok(())
}
