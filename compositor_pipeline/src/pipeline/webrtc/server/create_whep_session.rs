use crate::pipeline::webrtc::{
    error::WhipWhepServerError,
    handle_keyframe_requests::handle_keyframe_requests,
    whep_output::{
        peer_connection::PeerConnection, stream_media_to_peer::spawn_media_streaming_task,
    },
    WhipWhepServerState,
};
use axum::{
    body::Body,
    debug_handler,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use compositor_render::OutputId;
use std::sync::Arc;
use tracing::trace;
use uuid::Uuid;

#[debug_handler]
pub async fn handle_create_whep_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let output_id = OutputId(Arc::from(id.clone()));
    let session_id: Arc<str> = Arc::from(Uuid::new_v4().to_string()); // TODO maybe simple rand better
    trace!("SDP offer: {}", offer);

    validate_sdp_content_type(&headers)?;
    let outputs = state.outputs.clone();
    outputs.validate_token(&output_id, &headers).await?;
    let ctx = state.ctx.clone();

    let video_options = outputs.get_with(&output_id, |output| Ok(output.video_options.clone()))?;
    let audio_options = outputs.get_with(&output_id, |output| Ok(output.audio_options.clone()))?;

    let peer_connection = PeerConnection::new(&ctx.clone()).await?;

    let (video_track, video_sender) = match video_options.clone() {
        Some(opts) => {
            let (track, sender) = peer_connection.new_video_track(opts.encoder).await?;
            (Some(track), Some(sender))
        }
        None => (None, None),
    };

    let audio_track = match audio_options.clone() {
        Some(opts) => Some(peer_connection.new_audio_track(opts.encoder).await?),
        None => None,
    };

    let sdp_answer = peer_connection.negotiate_connection(offer).await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

    outputs.add_session(&output_id, session_id.clone(), Arc::new(peer_connection))?;

    spawn_media_streaming_task(
        ctx.clone(),
        &output_id,
        video_options.clone(),
        audio_options.clone(),
        video_track,
        audio_track,
    )
    .await;

    if let (Some(sender), Some(video_opt)) = (video_sender, video_options) {
        handle_keyframe_requests(
            &ctx.clone(),
            sender,
            video_opt
                .track_thread_handle
                .keyframe_request_sender
                .clone(),
        );
    }

    let body = Body::from(sdp_answer.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header(
            "Location",
            format!(
                "/whep/{}/{}",
                urlencoding::encode(&id),
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
