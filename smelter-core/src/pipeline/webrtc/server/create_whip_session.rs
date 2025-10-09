use crate::pipeline::{
    rtp::RtpNtpSyncPoint,
    webrtc::{
        WhipWhepServerState,
        error::WhipWhepServerError,
        peer_connection_recvonly::RecvonlyPeerConnection,
        whip_input::process_tracks::{process_audio_track, process_video_track},
    },
};
use axum::{
    body::Body,
    debug_handler,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use std::{sync::Arc, time::Duration};
use tracing::{Instrument, Level, debug, span, trace, warn};
use webrtc::{
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::RTPCodecType,
};

#[debug_handler]
pub async fn handle_create_whip_session(
    Path(endpoint_id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let endpoint_id = Arc::from(endpoint_id.clone());
    trace!("SDP offer: {}", offer);
    let inputs = state.inputs.clone();

    validate_sdp_content_type(&headers)?;
    inputs.validate_token(&endpoint_id, &headers).await?;

    let video_preferences =
        inputs.get_with(&endpoint_id, |input| Ok(input.video_preferences.clone()))?;

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
        let endpoint_id = endpoint_id.clone();
        let sync_point = RtpNtpSyncPoint::new(state.ctx.queue_sync_point);
        peer_connection.start_stats_monitor();
        peer_connection.on_track(Box::new(move |track, _, transceiver| {
            debug!(
                ?endpoint_id,
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
                            endpoint_id.clone(),
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
                            endpoint_id.clone(),
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
    let session_id = inputs.get_mut_with(&endpoint_id, |input| {
        input.maybe_replace_peer_connection(&endpoint_id, peer_connection)
    })?;

    let body = Body::from(sdp_answer.sdp.to_string());
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
