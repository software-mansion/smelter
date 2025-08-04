use crate::{
    event::Event,
    pipeline::{
        rtp::RtpPacket,
        webrtc::{
            error::WhipWhepServerError, peer_connection_sendonly::SendonlyPeerConnection,
            WhipWhepServerState,
        },
    },
    PipelineCtx,
};
use axum::{
    body::Body,
    debug_handler,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use compositor_render::OutputId;
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tracing::{debug, info, trace, warn};
use webrtc::{
    peer_connection::sdp::session_description::RTCSessionDescription,
    track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter},
};

#[debug_handler]
pub async fn handle_create_whep_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipWhepServerError> {
    let output_id = OutputId(Arc::from(id.clone()));
    info!("SDP offer: {}", offer);
    let outputs = state.outputs.clone();
    let ctx = state.ctx.clone();

    validate_sdp_content_type(&headers)?;
    outputs.validate_token(&output_id, &headers).await?;

    let video_encoder = outputs.get_with(&output_id, |output| Ok(output.video_encoder.clone()))?;
    let audio_encoder = outputs.get_with(&output_id, |output| Ok(output.audio_encoder.clone()))?;

    let peer_connection = SendonlyPeerConnection::new(&state.ctx).await?;

    let video_track = if let Some(encoder) = video_encoder {
        Some(peer_connection.new_video_track(encoder).await?)
    } else {
        None
    };
    let audio_track = if let Some(encoder) = audio_encoder {
        Some(peer_connection.new_audio_track(encoder).await?)
    } else {
        None
    };

    let offer = RTCSessionDescription::offer(offer)?;
    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer().await?;
    peer_connection.set_local_description(answer).await?;

    peer_connection
        .wait_for_ice_candidates(Duration::from_secs(1))
        .await?;

    let sdp_answer = peer_connection.local_description().await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

    let video_recv = state
        .outputs
        .get_with(&output_id, |v| Ok(v.video_receiver.clone()))?;
    let video_receiver = video_recv.map(|recv| recv.resubscribe());

    let audio_recv = state
        .outputs
        .get_with(&output_id, |v| Ok(v.audio_receiver.clone()))?;
    let audio_receiver = audio_recv.map(|recv| recv.resubscribe());

    let output_id_clone = output_id.clone();

    tokio::spawn(async move {
        run_send(
            ctx,
            &output_id_clone,
            video_receiver,
            audio_receiver,
            video_track,
            audio_track,
        )
        .await;
    });

    // // It will fail if there is already connected peer connection
    // outputs.get_mut_with(&output_id, |output| {
    //     output.maybe_replace_peer_connection(&output_id, peer_connection)
    // })?;

    let body = Body::from(sdp_answer.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header("Access-Control-Expose-Headers", "Location")
        // .header("Location", format!("/session/{}", urlencoding::encode(&id)))
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

async fn run_send(
    ctx: Arc<PipelineCtx>,
    output_id: &OutputId,
    mut video_receiver: Option<broadcast::Receiver<RtpPacket>>,
    mut audio_receiver: Option<broadcast::Receiver<RtpPacket>>,
    video_track: Option<Arc<TrackLocalStaticRTP>>,
    audio_track: Option<Arc<TrackLocalStaticRTP>>,
) {
    let mut next_video_packet = None;
    let mut next_audio_packet = None;

    loop {
        match (
            &next_video_packet,
            &next_audio_packet,
            &mut video_receiver,
            &mut audio_receiver,
        ) {
            (None, None, Some(video_receiver), Some(audio_receiver)) => {
                tokio::select! {
                    Ok(packet) = video_receiver.recv() => {
                        next_video_packet = Some(packet)
                    },
                    Ok(packet) = audio_receiver.recv() => {
                        next_audio_packet = Some(packet)
                    },
                    else => break,
                };
            }
            (_video, None, _video_receiver, audio_receiver @ Some(_)) => {
                match audio_receiver.as_mut().unwrap().recv().await {
                    Ok(packet) => {
                        next_audio_packet = Some(packet);
                    }
                    Err(_) => *audio_receiver = None,
                };
            }
            (None, _, video_receiver @ Some(_), _) => {
                match video_receiver.as_mut().unwrap().recv().await {
                    Ok(packet) => {
                        next_video_packet = Some(packet);
                    }
                    Err(_) => *video_receiver = None,
                };
            }
            (None, None, None, None) => {
                break;
            }
            (Some(_), Some(_), _, _) => {
                warn!("Both packets populated, this should not happen.");
            }
            (None, Some(_audio), None, _) => {
                // no video, but can't read audio at this moment
            }
            (Some(_video), None, _, None) => {
                // no audio, but can't read video at this moment
            }
        };

        match (&next_video_packet, &next_audio_packet) {
            // try to wait for both audio and video packet to be ready
            (Some(video), Some(audio)) => {
                if audio.timestamp > video.timestamp {
                    if let (Some(packet), Some(track)) = (next_video_packet.take(), &video_track) {
                        if let Err(err) = track.write_rtp(&packet.packet).await {
                            warn!("RTP write error {}", err);
                            break;
                        }
                    }
                } else if let (Some(packet), Some(track)) = (next_audio_packet.take(), &audio_track)
                {
                    if let Err(err) = track.write_rtp(&packet.packet).await {
                        warn!("RTP write error {}", err);
                        break;
                    }
                }
            }
            // read audio if there is not way to get video packet
            (None, Some(_)) if video_receiver.is_none() => {
                if let (Some(p), Some(track)) = (next_audio_packet.take(), &audio_track) {
                    if let Err(err) = track.write_rtp(&p.packet).await {
                        warn!("RTP write error {}", err);
                        break;
                    }
                }
            }
            // read video if there is not way to get audio packet
            (Some(_), None) if audio_receiver.is_none() => {
                if let (Some(p), Some(track)) = (next_video_packet.take(), &video_track) {
                    if let Err(err) = track.write_rtp(&p.packet).await {
                        warn!("RTP write error {}", err);
                        break;
                    }
                }
            }
            (None, None) => break,
            // we can't do anything here, but there are still receivers
            // that can return something in the next loop.
            //
            // I don't think this can ever happen
            (_, _) => (),
        };
    }

    ctx.event_emitter.emit(Event::OutputDone(output_id.clone()));
    debug!("Closing WHEP sender thread.")
}
