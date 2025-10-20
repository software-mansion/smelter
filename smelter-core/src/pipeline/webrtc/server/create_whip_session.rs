use std::sync::Arc;

use crate::pipeline::webrtc::{
    WhipWhepServerState, error::WhipWhepServerError,
    whip_input::create_new_session::create_new_whip_session,
};
use axum::{
    body::Body,
    debug_handler,
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
};
use reqwest::StatusCode;
use tracing::debug;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

#[debug_handler]
pub async fn handle_create_whip_session(
    Path(endpoint_id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let endpoint_id = Arc::from(endpoint_id.clone());
    debug!("SDP offer: {}", offer);

    let input_id = state.inputs.find_by_endpoint_id(&endpoint_id)?;

    validate_sdp_content_type(&headers)?;
    state.inputs.validate_token(&input_id, &headers).await?;

    let offer = RTCSessionDescription::offer(offer)?;

    let (session_id, answer) = create_new_whip_session(state, input_id, offer).await?;

    let body = Body::from(answer.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header("Access-Control-Expose-Headers", "Location")
        .header(
            "Location",
            format!(
                "/whip/{}/{}",
                urlencoding::encode(&endpoint_id),
                urlencoding::encode(&session_id)
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
