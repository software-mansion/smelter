use crate::pipeline::webrtc::{
    error::WhipWhepServerError,
    handle_keyframe_requests::handle_keyframe_requests,
    whep_output::{peer_connection::PeerConnection, stream_media_to_peer::stream_media_to_peer},
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

#[debug_handler]
pub async fn handle_create_whep_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let output_id = OutputId(Arc::from(id.clone()));
    trace!("SDP offer: {}", offer);

    validate_sdp_content_type(&headers)?;
    let outputs = state.outputs.clone();
    outputs.validate_token(&output_id, &headers).await?;
    let ctx = state.ctx.clone();

    let (video_encoder, video_receiver, keyframe_request_sender) =
        outputs.get_with(&output_id, |output| {
            if let Some(v) = &output.video_options {
                Ok((
                    Some(v.encoder.clone()),
                    Some(v.receiver.resubscribe()),
                    Some(v.track_thread_handle.keyframe_request_sender.clone()),
                ))
            } else {
                Ok((None, None, None))
            }
        })?;

    let (audio_encoder, audio_receiver) = outputs.get_with(&output_id, |output| {
        if let Some(a) = &output.audio_options {
            Ok((Some(a.encoder.clone()), Some(a.receiver.resubscribe())))
        } else {
            Ok((None, None))
        }
    })?;

    let peer_connection =
        PeerConnection::new(&ctx.clone(), video_encoder.clone(), audio_encoder.clone()).await?;

    let (video_track, video_sender) = match video_encoder {
        Some(encoder) => {
            let (track, sender) = peer_connection.new_video_track(encoder).await?;
            (Some(track), Some(sender))
        }
        None => (None, None),
    };

    let audio_track = match audio_encoder {
        Some(encoder) => Some(peer_connection.new_audio_track(encoder).await?),
        None => None,
    };

    let sdp_answer = peer_connection.negotiate_connection(offer).await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

    let ctx_clone = ctx.clone();
    tokio::spawn(async move {
        stream_media_to_peer(
            ctx_clone,
            &output_id,
            video_receiver,
            audio_receiver,
            video_track,
            audio_track,
        )
        .await;
    });

    if let (Some(sender), Some(keyframe_request_sender)) = (video_sender, keyframe_request_sender) {
        handle_keyframe_requests(&ctx.clone(), sender, keyframe_request_sender);
    }

    let body = Body::from(sdp_answer.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header(
            "Location",
            format!("/resource/{}", urlencoding::encode(&id)),
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
