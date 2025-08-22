use crate::pipeline::webrtc::{
    error::WhipWhepServerError,
    handle_keyframe_requests::handle_keyframe_requests,
    whep_output::{
        init_payloaders::{init_audio_payloader, init_video_payloader},
        peer_connection::PeerConnection,
        stream_media_to_peer::{stream_media_to_peer, MediaStream},
    },
    WhipWhepServerState,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use compositor_render::OutputId;
use std::sync::Arc;
use tracing::trace;

pub async fn handle_create_whep_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let output_id = OutputId(Arc::from(id.clone()));
    trace!("SDP offer: {}", offer);

    validate_sdp_content_type(&headers)?;
    let outputs = state.outputs;
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

    let peer_connection = PeerConnection::new(&ctx, &video_encoder, &audio_encoder).await?;

    let (video_media_stream, video_sender) = match (&video_encoder, video_receiver) {
        (Some(encoder), Some(receiver)) => {
            let (track, sender, ssrc) = peer_connection.new_video_track(encoder).await?;
            let payloader = init_video_payloader(encoder, ssrc);
            (
                Some(MediaStream {
                    receiver,
                    track,
                    payloader,
                }),
                Some(sender),
            )
        }
        _ => (None, None),
    };

    let audio_media_stream = match (&audio_encoder, audio_receiver) {
        (Some(encoder), Some(receiver)) => {
            let (track, ssrc) = peer_connection.new_audio_track(encoder).await?;
            let payloader = init_audio_payloader(ssrc);
            Some(MediaStream {
                receiver,
                track,
                payloader,
            })
        }
        _ => None,
    };

    let sdp_answer = peer_connection.negotiate_connection(offer).await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

    let session_id = outputs.add_session(&output_id, peer_connection.clone())?;
    peer_connection.attach_cleanup_when_pc_failed(
        outputs.clone(),
        output_id.clone(),
        session_id.clone(),
    );

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
