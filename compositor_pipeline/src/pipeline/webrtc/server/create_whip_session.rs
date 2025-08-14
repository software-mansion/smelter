use crate::pipeline::{
    rtp::RtpNtpSyncPoint,
    webrtc::{
        error::WhipWhepServerError,
        peer_connection_recvonly::RecvonlyPeerConnection,
        whip_input::{
            track_audio_thread::process_audio_track, track_video_thread::process_video_track,
        },
        WhipWhepServerState,
    },
};
use axum::{
    body::Body,
    debug_handler,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use std::{sync::Arc, time::Duration};
use tracing::{debug, span, trace, warn, Instrument, Level};
use webrtc::{
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::RTPCodecType,
};

#[debug_handler]
pub async fn handle_create_whip_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let session_id = Arc::from(id.clone());
    trace!("SDP offer: {}", offer);
    let inputs = state.inputs.clone();

    validate_sdp_content_type(&headers)?;
    inputs.validate_token(&session_id, &headers).await?;

    let video_preferences =
        inputs.get_with(&session_id, |input| Ok(input.video_preferences.clone()))?;

    let peer_connection = RecvonlyPeerConnection::new(&state.ctx, &video_preferences).await?;

    let _video_transceiver = peer_connection.new_video_track(&video_preferences).await?;
    let _audio_transceiver = peer_connection.new_audio_track().await?;

    let offer = RTCSessionDescription::offer(offer)?;
    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer().await?;
    peer_connection.set_local_description(answer).await?;

    peer_connection
        .wait_for_ice_candidates(Duration::from_secs(1))
        .await?;

    let sdp_answer = peer_connection.local_description().await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

    {
        let session_id = session_id.clone();
        let sync_point = RtpNtpSyncPoint::new(state.ctx.queue_sync_point);
        peer_connection.on_track(Box::new(move |track, _, transceiver| {
            debug!(
                ?session_id,
                kind=?track.kind(),
                "on_track called"
            );

            let span = span!(Level::INFO, "WHIP input track", track_type =? track.kind());

            match track.kind() {
                RTPCodecType::Audio => {
                    tokio::spawn(
                        process_audio_track(
                            sync_point.clone(),
                            state.clone(),
                            session_id.clone(),
                            track,
                            transceiver,
                        )
                        .instrument(span),
                    );
                }
                RTPCodecType::Video => {
                    tokio::spawn(
                        process_video_track(
                            sync_point.clone(),
                            state.clone(),
                            session_id.clone(),
                            track,
                            transceiver,
                            video_preferences.clone(),
                        )
                        .instrument(span),
                    );
                }
                RTPCodecType::Unspecified => {
                    warn!("Unknown track kind")
                }
            }

            Box::pin(async {})
        }))
    };

    // It will fail if there is already connected peer connection
    inputs.get_mut_with(&session_id, |input| {
        input.maybe_replace_peer_connection(&session_id, peer_connection)
    })?;

    let body = Body::from(sdp_answer.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header("Access-Control-Expose-Headers", "Location")
        .header("Location", format!("/whip/{}", urlencoding::encode(&id)))
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
