use crate::{
    codecs::VideoEncoderOptions,
    event::Event,
    pipeline::{
        encoder::{
            encoder_thread_video::spawn_video_encoder_thread, ffmpeg_h264::FfmpegH264Encoder,
            ffmpeg_vp8::FfmpegVp8Encoder, ffmpeg_vp9::FfmpegVp9Encoder,
        },
        rtp::payloader::{PayloadedCodec, PayloaderOptions},
        webrtc::{
            error::WhipWhepServerError, peer_connection_sendonly::SendonlyPeerConnection,
            whep_output::WhepSenderTrack, WhipWhepServerState,
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
use crossbeam_channel::bounded;
use ffmpeg_next::format::Output;
use std::{sync::Arc, time::Duration};
use tracing::{debug, info, trace, warn};
use webrtc::{
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::RTPCodecType, track::track_local::TrackLocalWriter,
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

    let video_encoder = outputs.get_with(&output_id, |output| {
        Ok(output.video_encoder.clone().unwrap())
    })?;
    let audio_encoder = outputs.get_with(&output_id, |output| {
        Ok(output.audio_encoder.clone().unwrap())
    })?;

    let peer_connection = SendonlyPeerConnection::new(&state.ctx).await?;

    peer_connection.new_video_track().await?;
    peer_connection.new_audio_track().await?;

    let offer = RTCSessionDescription::offer(offer)?;
    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer().await?;
    peer_connection.set_local_description(answer).await?;

    peer_connection
        .wait_for_ice_candidates(Duration::from_secs(1))
        .await?;

    let sdp_answer = peer_connection.local_description().await?;
    trace!("SDP answer: {}", sdp_answer.sdp);

    fn payloader_options(codec: PayloadedCodec, payload_type: u8, ssrc: u32) -> PayloaderOptions {
        PayloaderOptions {
            codec,
            payload_type,
            clock_rate: 48_000,
            mtu: 1200,
            ssrc,
        }
    }

    {
        let output_id = output_id.clone();
        println!("{output_id:?}");
        peer_connection.on_track(Box::new(move |track, _, transceiver| {
            debug!(
                kind = track.kind().to_string(),
                ?output_id,
                "on_track called"
            );
            println!("track-kind   {:?}", track.kind());

            let (sender, encoder) = crossbeam_channel::bounded(1);

            match track.kind() {
                RTPCodecType::Audio => {
                    // spawn_audio_track_thread(
                    //     state.ctx,
                    //     output_id,
                    //     audio_encoder,
                    //     payloader_options(PayloadedCodec::Opus, 97, 1),
                    //     sender,
                    // );
                }
                RTPCodecType::Video => {
                    let encoder = match &video_encoder {
                        VideoEncoderOptions::FfmpegH264(options) => {
                            spawn_video_encoder_thread::<FfmpegH264Encoder>(
                                ctx.clone(),
                                output_id.clone(),
                                options.clone(),
                                sender.clone(),
                            )
                        }
                        VideoEncoderOptions::FfmpegVp8(options) => {
                            spawn_video_encoder_thread::<FfmpegVp8Encoder>(
                                ctx.clone(),
                                output_id.clone(),
                                options.clone(),
                                sender.clone(),
                            )
                        }
                        VideoEncoderOptions::FfmpegVp9(options) => {
                            spawn_video_encoder_thread::<FfmpegVp9Encoder>(
                                ctx.clone(),
                                output_id.clone(),
                                options.clone(),
                                sender.clone(),
                            )
                        }
                    };

                    // tokio::spawn(process_video_track(
                    //     state.clone(),
                    //     output_id.clone(),
                    //     track,
                    //     transceiver,
                    //     video_preferences.clone(),
                    // ));
                }
                RTPCodecType::Unspecified => {
                    warn!("Unknown track kind")
                }
            };

            Box::pin(async {})
        }))
    };

    // It will fail if there is already connected peer connection
    outputs.get_mut_with(&output_id, |output| {
        output.maybe_replace_peer_connection(&output_id, peer_connection)
    })?;

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
    ctx: PipelineCtx,
    output_id: OutputId,
    video_track: Option<WhepSenderTrack>,
    audio_track: Option<WhepSenderTrack>,
) {
    let (mut audio_receiver, audio_track) = match audio_track {
        Some(WhepSenderTrack { receiver, track }) => (Some(receiver), Some(track)),
        None => (None, None),
    };

    let (mut video_receiver, video_track) = match video_track {
        Some(WhepSenderTrack { receiver, track }) => (Some(receiver), Some(track)),
        None => (None, None),
    };
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
                    Some(packet) = video_receiver.recv() => {
                        next_video_packet = Some(packet)
                    },
                    Some(packet) = audio_receiver.recv() => {
                        next_audio_packet = Some(packet)
                    },
                    else => break,
                };
            }
            (_video, None, _video_receiver, audio_receiver @ Some(_)) => {
                match audio_receiver.as_mut().unwrap().recv().await {
                    Some(packet) => {
                        next_audio_packet = Some(packet);
                    }
                    None => *audio_receiver = None,
                };
            }
            (None, _, video_receiver @ Some(_), _) => {
                match video_receiver.as_mut().unwrap().recv().await {
                    Some(packet) => {
                        next_video_packet = Some(packet);
                    }
                    None => *video_receiver = None,
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

    ctx.event_emitter.emit(Event::OutputDone(output_id));
    debug!("Closing WHIP sender thread.")
}
