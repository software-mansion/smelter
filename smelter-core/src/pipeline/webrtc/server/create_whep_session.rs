use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use tracing::debug;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::pipeline::webrtc::{
    WhipWhepServerState, error::WhipWhepServerError,
    whep_output::create_new_session::create_new_whep_session,
};

pub async fn handle_create_whep_session(
    Path(output_id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let output_ref = state.outputs.resolve_output_ref(&output_id)?;
    debug!("SDP offer: {}", offer);

    validate_sdp_content_type(&headers)?;
    state.outputs.validate_token(&output_ref, &headers)?;

    let offer = RTCSessionDescription::offer(offer)?;
    let (session_id, answer) = create_new_whep_session(state, output_ref, offer).await?;

    let body = Body::from(answer.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header(
            "Location",
            format!(
                "/whep/{}/{}",
                urlencoding::encode(&output_id),
                urlencoding::encode(&session_id),
            ),
        )
        .body(body)?;
    Ok(response)
}

pub fn validate_sdp_content_type(headers: &HeaderMap) -> Result<(), WhipWhepServerError> {
    if let Some(content_type) = headers.get("Content-Type") {
        if content_type.as_bytes() != b"application/sdp" {
            return Err(WhipWhepServerError::InternalError(
                "Invalid Content-Type".to_string(),
            ));
        }
    } else {
        return Err(WhipWhepServerError::BadRequest(
            "Missing Content-Type header".to_string(),
        ));
    }
    Ok(())
}
