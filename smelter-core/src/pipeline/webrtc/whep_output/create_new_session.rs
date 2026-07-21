use std::sync::Arc;

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

use crate::prelude::*;

pub async fn create_new_whep_session(
    state: WhipWhepServerState,
    output_ref: Ref<OutputId>,
    offer: RTCSessionDescription,
) -> Result<(Arc<str>, RTCSessionDescription), WhipWhepServerError> {
    let session_id: Arc<str> = Arc::from(Uuid::new_v4().to_string());

    let (video_encoder, video_receiver, keyframe_request_sender) =
        state.outputs.get_with(&output_ref, |output| {
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

    let (audio_encoder, audio_receiver) = state.outputs.get_with(&output_ref, |output| {
        if let Some(a) = &output.audio_options {
            Ok((Some(a.encoder.clone()), Some(a.receiver.resubscribe())))
        } else {
            Ok((None, None))
        }
    })?;

    let peer_connection =
        PeerConnection::new(&state.ctx, &video_encoder, &audio_encoder, &offer).await?;

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

    let pc_state_hdlr =
        ConnectionStateChangeHdlr::new(&state.ctx, &output_ref, &session_id, &state.outputs);
    peer_connection.on_connection_state_change(pc_state_hdlr);

    let sdp_answer = peer_connection
        .negotiate_connection(offer, video_sender.clone(), audio_sender.clone())
        .await?;
    debug!("SDP answer: {}", sdp_answer.sdp);

    if let (Some(sender), Some(keyframe_request_sender)) = (video_sender, keyframe_request_sender) {
        handle_keyframe_requests(&state.ctx.clone(), sender, keyframe_request_sender);
    }

    state
        .outputs
        .add_session(&output_ref, &session_id, peer_connection)?;

    tokio::spawn(stream_media_to_peer(
        state.ctx.clone(),
        output_ref,
        video_media_stream,
        audio_media_stream,
    ));

    Ok((session_id, sdp_answer))
}
