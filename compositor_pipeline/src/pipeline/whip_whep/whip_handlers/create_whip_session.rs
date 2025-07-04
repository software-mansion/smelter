use crate::pipeline::{
    input::whip::{process_track_stream, start_decoders::start_decoders_threads},
    whip_whep::{
        bearer_token::validate_token,
        error::WhipServerError,
        init_peer_connection,
        supported_video_codec_parameters::{
            get_video_h264_codecs_for_codec_preferences, get_video_vp8_codecs, get_video_vp9_codecs,
        },
        WhipWhepServerState,
    },
    VideoDecoder,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode},
};
use compositor_render::InputId;
use init_peer_connection::init_peer_connection;
use std::{sync::Arc, time::Duration};
use tokio::{sync::watch, time::timeout};
use tracing::{debug, info, trace, warn};
use urlencoding::encode;
use webrtc::{
    ice_transport::ice_gatherer_state::RTCIceGathererState,
    peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection},
    rtp_transceiver::rtp_codec::RTCRtpCodecParameters,
};

pub async fn handle_create_whip_session(
    Path(id): Path<String>,
    State(state): State<WhipWhepServerState>,
    headers: HeaderMap,
    offer: String,
) -> Result<Response<Body>, WhipServerError> {
    let input_id = InputId(Arc::from(id.clone()));

    validate_sdp_content_type(&headers)?;

    trace!("SDP offer: {}", offer);

    let input_state = state
        .inputs
        .get_input_connection_options(input_id.clone())?;

    validate_token(input_state.bearer_token, headers.get("Authorization")).await?;

    //Deleting previous peer_connection on this input which was not in Connected state
    if let Some(connection) = input_state.peer_connection {
        if let Err(err) = connection.close().await {
            return Err(WhipServerError::InternalError(format!(
                "Cannot close previously existing peer connection {input_id:?}: {err:?}"
            )));
        }
    }

    let (peer_connection, video_transceiver, audio_transceiver) = init_peer_connection(
        state.ctx.stun_servers.to_vec(),
        input_state.video_decoder_preferences.clone(),
    )
    .await?;

    if let Err(err) = video_transceiver
        .set_codec_preferences(map_video_decoder_to_rtp_codec_parameters(
            input_state.video_decoder_preferences.clone(),
        ))
        .await
    {
        warn!("Cannot set codec preferences for sdp answer: {err:?}");
    }

    state
        .inputs
        .update_peer_connection(input_id.clone(), peer_connection.clone())
        .await?;

    peer_connection.on_ice_connection_state_change(Box::new(move |state| {
        info!("ICE connection state changed: {state:?}");
        Box::pin(async {})
    }));

    let description = RTCSessionDescription::offer(offer)?;

    peer_connection.set_remote_description(description).await?;

    let payload_type_map = start_decoders_threads(
        &state.ctx,
        &state.inputs,
        input_id.clone(),
        video_transceiver.clone(),
        audio_transceiver,
        input_state.video_decoder_preferences.clone(),
    )
    .await?;

    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;
    gather_ice_candidates_for_one_second(peer_connection.clone()).await;

    let Some(sdp) = peer_connection.local_description().await else {
        return Err(WhipServerError::InternalError(
            "Local description is not set, cannot read it".to_string(),
        ));
    };
    trace!("Sending SDP answer: {sdp:?}");

    peer_connection.on_track(Box::new(move |track, _, _| {
        //tokio::spawn is necessary to concurrently process audio and video track
        tokio::spawn(process_track_stream(
            track,
            state.inputs.clone(),
            input_id.clone(),
            payload_type_map.clone(),
        ));
        Box::pin(async {})
    }));

    let body = Body::from(sdp.sdp.to_string());
    let response = Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/sdp")
        .header("Access-Control-Expose-Headers", "Location")
        .header("Location", format!("/session/{}", encode(&id)))
        .body(body)?;
    Ok(response)
}

pub fn validate_sdp_content_type(headers: &HeaderMap) -> Result<(), WhipServerError> {
    if let Some(content_type) = headers.get("Content-Type") {
        if content_type.as_bytes() != b"application/sdp" {
            return Err(WhipServerError::InternalError(
                "Invalid Content-Type".to_string(),
            ));
        }
    } else {
        return Err(WhipServerError::BadRequest(
            "Missing Content-Type header".to_string(),
        ));
    }
    Ok(())
}

pub async fn gather_ice_candidates_for_one_second(peer_connection: Arc<RTCPeerConnection>) {
    let (sender, mut receiver) = watch::channel(RTCIceGathererState::Unspecified);

    peer_connection.on_ice_gathering_state_change(Box::new(move |gatherer_state| {
        if let Err(err) = sender.send(gatherer_state) {
            debug!("Cannot send gathering state: {err:?}");
        };
        Box::pin(async {})
    }));

    let gather_candidates = async {
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() == RTCIceGathererState::Complete {
                break;
            }
        }
    };

    if let Err(err) = timeout(Duration::from_secs(1), gather_candidates).await {
        debug!("Maximum time for gathering candidate has elapsed: {err:?}");
    }
}

fn map_video_decoder_to_rtp_codec_parameters(
    video_decoder_preferences: Vec<VideoDecoder>,
) -> Vec<RTCRtpCodecParameters> {
    let video_vp8_codec = get_video_vp8_codecs();
    let video_vp9_codec = get_video_vp9_codecs();
    let video_h264_codecs = get_video_h264_codecs_for_codec_preferences();

    let mut codec_list = Vec::new();

    for decoder in video_decoder_preferences {
        match decoder {
            VideoDecoder::FFmpegH264 => {
                codec_list.extend(video_h264_codecs.clone());
            }
            #[cfg(feature = "vk-video")]
            VideoDecoder::VulkanVideoH264 => {
                codec_list.extend(video_h264_codecs.clone());
            }
            VideoDecoder::FFmpegVp8 => {
                codec_list.extend(video_vp8_codec.clone());
            }
            VideoDecoder::FFmpegVp9 => {
                codec_list.extend(video_vp9_codec.clone());
            }
        }
    }

    codec_list
}
