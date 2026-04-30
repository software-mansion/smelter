use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use tracing::debug;
use uuid::Uuid;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::pipeline::webrtc::{
    WhipWhepServerState,
    error::WhipWhepServerError,
    handle_keyframe_requests::handle_keyframe_requests,
    whep_output::{
        init_payloaders::{init_audio_payloader, init_video_payloader},
        pc_state_change::ConnectionStateChangeHdlr,
        peer_connection::PeerConnection,
        stream_media_to_peer::{MediaStream, stream_media_to_peer},
    },
};

pub async fn handle_create_whep_session(
    Path(endpoint_id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let endpoint_id = Arc::from(endpoint_id.clone());
    let output_ref = state.outputs.find_by_endpoint_id(&endpoint_id)?;
    let session_id: Arc<str> = Arc::from(Uuid::new_v4().to_string());
    debug!("SDP offer: {}", offer);

    validate_sdp_content_type(&headers)?;
    let outputs = state.outputs;
    outputs.validate_token(&output_ref, &headers).await?;
    let ctx = state.ctx.clone();

    let (video_encoder, video_receiver, keyframe_request_sender) =
        outputs.get_with(&output_ref, |output| {
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

    let (audio_encoder, audio_receiver) = outputs.get_with(&output_ref, |output| {
        if let Some(a) = &output.audio_options {
            Ok((Some(a.encoder.clone()), Some(a.receiver.resubscribe())))
        } else {
            Ok((None, None))
        }
    })?;

    let parsed_offer = RTCSessionDescription::offer(offer)?;

    let peer_connection =
        PeerConnection::new(&ctx, &video_encoder, &audio_encoder, &parsed_offer).await?;

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

    let (audio_media_stream, audio_sender) = match (&audio_encoder, audio_receiver) {
        (Some(encoder), Some(receiver)) => {
            let (track, sender, ssrc) = peer_connection.new_audio_track(encoder).await?;
            let payloader = init_audio_payloader(ssrc);
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

    let pc_state_hdlr = ConnectionStateChangeHdlr::new(&ctx, &output_ref, &session_id, &outputs);
    peer_connection.on_connection_state_change(pc_state_hdlr);

    let sdp_answer = peer_connection
        .negotiate_connection(parsed_offer, video_sender.clone(), audio_sender.clone())
        .await?;
    debug!("SDP answer: {}", sdp_answer.sdp);

    if let (Some(sender), Some(keyframe_request_sender)) = (video_sender, keyframe_request_sender) {
        handle_keyframe_requests(&ctx.clone(), sender, keyframe_request_sender);
    }

    outputs.add_session(&output_ref, &session_id, peer_connection)?;

    ctx.spawn_tracked(stream_media_to_peer(
        ctx.clone(),
        output_ref,
        video_media_stream,
        audio_media_stream,
    ));

    let body = Body::from(sdp_answer.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header(
            "Location",
            format!(
                "/whep/{}/{}",
                urlencoding::encode(&endpoint_id),
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
