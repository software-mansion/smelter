use crate::pipeline::webrtc::{
    error::WhipWhepServerError,
    handle_keyframe_requests::handle_keyframe_requests,
    whep_output::{
        init_payloaders::init_payloaders,
        peer_connection::PeerConnection,
        stream_media_to_peer::{stream_media_to_peer, MediaStream},
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

    let (video_track, video_sender, video_ssrc) = match video_encoder.clone() {
        Some(encoder) => {
            let (track, sender, ssrc) = peer_connection.new_video_track(encoder).await?;

            (Some(track), Some(sender), Some(ssrc))
        }
        None => (None, None, None),
    };

    let (audio_track, audio_ssrc) = match audio_encoder.clone() {
        Some(encoder) => {
            let (track, ssrc) = peer_connection.new_audio_track(encoder).await?;
            (Some(track), Some(ssrc))
        }
        None => (None, None),
    };

    let sdp_answer = peer_connection.negotiate_connection(offer).await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

    let session_id = outputs.add_session(&output_id, Arc::new(peer_connection))?;

    let (video_payloader, audio_payloader) = init_payloaders(
        video_encoder.clone(),
        audio_encoder.clone(),
        video_ssrc,
        audio_ssrc,
    );

    let video_media_stream = MediaStream {
        receiver: video_receiver,
        track: video_track,
        payloader: video_payloader,
    };

    let audio_media_stream = MediaStream {
        receiver: audio_receiver,
        track: audio_track,
        payloader: audio_payloader,
    };

    tokio::spawn(stream_media_to_peer(
        ctx.clone(),
        output_id,
        video_media_stream,
        audio_media_stream,
    ));

    if let (Some(sender), Some(keyframe_request_sender)) = (video_sender, keyframe_request_sender) {
        handle_keyframe_requests(&ctx.clone(), sender, keyframe_request_sender);
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
