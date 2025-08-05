use crate::{
    event::Event,
    pipeline::{
        rtp::RtpPacket,
        webrtc::{
            error::WhipWhepServerError,
            handle_keyframe_requests::handle_keyframe_requests,
            peer_connection_sendonly::SendonlyPeerConnection,
            whep_output::connection_state::{
                WhepAudioConnectionOptions, WhepVideoConnectionOptions,
            },
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
use tokio::sync::{broadcast, watch};
use tracing::{debug, error, span, trace, warn, Instrument, Level};
use webrtc::{
    rtp_transceiver::rtp_sender::RTCRtpSender,
    stats::StatsReportType,
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
    trace!("SDP offer: {}", offer);

    validate_sdp_content_type(&headers)?;
    let outputs = state.outputs.clone();
    outputs.validate_token(&output_id, &headers).await?;
    let ctx = state.ctx.clone();

    let video_options = outputs.get_with(&output_id, |output| Ok(output.video_options.clone()))?;
    let audio_options = outputs.get_with(&output_id, |output| Ok(output.audio_options.clone()))?;

    let peer_connection = SendonlyPeerConnection::new(&ctx.clone()).await?;

    let (video_track, video_sender) = match video_options.clone() {
        Some(opts) => {
            let (track, sender) = peer_connection.new_video_track(opts.encoder).await?;
            (Some(track), Some(sender))
        }
        None => (None, None),
    };

    let (audio_track, audio_sender) = match audio_options.clone() {
        Some(opts) => {
            let (track, sender) = peer_connection.new_audio_track(opts.encoder).await?;
            (Some(track), Some(sender))
        }
        None => (None, None),
    };

    let sdp_answer = peer_connection.negotiate_connection(offer).await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

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

    if let (Some(sender), Some(audio_opt)) = (audio_sender, audio_options) {
        handle_packet_loss_requests(
            &ctx,
            peer_connection,
            sender.clone(),
            audio_opt.track_thread_handle.packet_loss_sender.clone(),
            audio_opt.ssrc,
        );
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

// TODO split into seperate files

async fn spawn_media_streaming_task(
    ctx: Arc<PipelineCtx>,
    output_id: &OutputId,
    video_options: Option<WhepVideoConnectionOptions>,
    audio_options: Option<WhepAudioConnectionOptions>,
    video_track: Option<Arc<TrackLocalStaticRTP>>,
    audio_track: Option<Arc<TrackLocalStaticRTP>>,
) {
    let video_receiver = video_options
        .as_ref()
        .map(|opts| opts.receiver.resubscribe());

    let audio_receiver = audio_options
        .as_ref()
        .map(|opts| opts.receiver.resubscribe());

    let output_id_clone = output_id.clone();

    tokio::spawn(async move {
        stream_media_to_peer(
            ctx,
            &output_id_clone,
            video_receiver,
            audio_receiver,
            video_track,
            audio_track,
        )
        .await;
    });
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

async fn stream_media_to_peer(
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

// Identifiers used in stats HashMap returnet by RTCPeerConnection::get_stats()
const RTC_OUTBOUND_RTP_AUDIO_STREAM: &str = "RTCOutboundRTPAudioStream_";
const RTC_REMOTE_INBOUND_RTP_AUDIO_STREAM: &str = "RTCRemoteInboundRTPAudioStream_";

fn handle_packet_loss_requests(
    ctx: &Arc<PipelineCtx>,
    pc: SendonlyPeerConnection,
    rtc_sender: Arc<RTCRtpSender>,
    packet_loss_sender: watch::Sender<i32>,
    ssrc: u32,
) {
    let mut cumulative_packets_sent: u64 = 0;
    let mut cumulative_packets_lost: u64 = 0;

    let span = span!(Level::DEBUG, "Packet loss handle");

    ctx.tokio_rt.spawn(
        async move {
            loop {
                if let Err(e) = rtc_sender.read_rtcp().await {
                    debug!(%e, "Error while reading rtcp.");
                }
            }
        }
        .instrument(span.clone()),
    );

    ctx.tokio_rt.spawn(
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let stats = pc.get_stats().await.reports;
                let outbound_id = String::from(RTC_OUTBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();
                let remote_inbound_id =
                    String::from(RTC_REMOTE_INBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();

                let outbound_stats = match stats.get(&outbound_id) {
                    Some(StatsReportType::OutboundRTP(report)) => report,
                    Some(_) => {
                        error!("Invalid report type for given key! (This should not happen)");
                        continue;
                    }
                    None => {
                        debug!("OutboundRTP report is empty!");
                        continue;
                    }
                };

                let remote_inbound_stats = match stats.get(&remote_inbound_id) {
                    Some(StatsReportType::RemoteInboundRTP(report)) => report,
                    Some(_) => {
                        error!("Invalid report type for given key! (This should not happen)");
                        continue;
                    }
                    None => {
                        debug!("RemoteInboundRTP report is empty!");
                        continue;
                    }
                };

                let packets_sent: u64 = outbound_stats.packets_sent;
                // This can be lower than 0 in case of duplicates
                let packets_lost: u64 = i64::max(remote_inbound_stats.packets_lost, 0) as u64;

                let packet_loss_percentage = calculate_packet_loss_percentage(
                    packets_sent,
                    packets_lost,
                    cumulative_packets_sent,
                    cumulative_packets_lost,
                );
                if packet_loss_sender.send(packet_loss_percentage).is_err() {
                    debug!("Packet loss channel closed.");
                }
                cumulative_packets_sent = packets_sent;
                cumulative_packets_lost = packets_lost;
            }
        }
        .instrument(span),
    );
}

fn calculate_packet_loss_percentage(
    packets_sent: u64,
    packets_lost: u64,
    cumulative_packets_sent: u64,
    cumulative_packets_lost: u64,
) -> i32 {
    let packets_sent_since_last_report = packets_sent - cumulative_packets_sent;
    let packets_lost_since_last_report = packets_lost - cumulative_packets_lost;

    // I don't want the system to panic in case of some bug
    let packet_loss_percentage: i32 = if packets_sent_since_last_report != 0 {
        let mut loss =
            100.0 * packets_lost_since_last_report as f64 / packets_sent_since_last_report as f64;
        // loss is rounded up to the nearest multiple of 5
        loss = f64::ceil(loss / 5.0) * 5.0;
        loss as i32
    } else {
        0
    };

    trace!(
        packets_sent_since_last_report,
        packets_lost_since_last_report,
        packet_loss_percentage,
    );
    packet_loss_percentage
}
