use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing::debug;
use uuid::Uuid;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::{
    pipeline::{
        rtp::{RtpJitterBufferMode, RtpJitterBufferSharedContext},
        webrtc::{
            WhipWhepServerState,
            error::WhipWhepServerError,
            offer_codec_filter::codecs_from_offer,
            peer_connection_recvonly::RecvonlyPeerConnection,
            whip_input::{
                WhipTrackContext, on_track::handle_on_track, state::WhipInputSession,
                video_preferences::video_params_compliant_with_offer,
            },
        },
    },
    queue::{QueueTrackOffset, QueueTrackOptions},
};

use crate::prelude::*;

pub(crate) async fn create_new_whip_session(
    state: WhipWhepServerState,
    input_ref: Ref<InputId>,
    offer: RTCSessionDescription,
) -> Result<(Arc<str>, RTCSessionDescription), WhipWhepServerError> {
    let inputs = state.inputs.clone();

    let (queue_input, video_preferences) = inputs.get_with(&input_ref, |input| {
        Ok((input.queue_input.upgrade(), input.video_preferences.clone()))
    })?;
    let Some(queue_input) = queue_input else {
        return Err(WhipWhepServerError::NotFound(format!(
            "Input {input_ref} not found"
        )));
    };

    let offer_codecs = codecs_from_offer(&offer);
    let video_codecs =
        video_params_compliant_with_offer(&state.ctx, &video_preferences, &offer_codecs);

    let peer_connection =
        RecvonlyPeerConnection::new(&state.ctx, &video_codecs, &offer_codecs.opus).await?;

    let _video_transceiver = peer_connection.new_video_track(&video_codecs).await?;
    let _audio_transceiver = peer_connection.new_audio_track().await?;

    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer().await?;
    peer_connection.set_local_description(answer).await?;

    peer_connection
        .wait_for_ice_candidates(Duration::from_secs(1))
        .await?;

    let answer = peer_connection.local_description().await.ok_or_else(|| {
        WhipWhepServerError::InternalError(
            "Local description is not set, cannot read it".to_string(),
        )
    })?;
    debug!("SDP answer: {}", answer.sdp);

    let weak_pear_connection = peer_connection.downgrade();
    let session_id: Arc<str> = Arc::from(Uuid::new_v4().to_string());
    // It will fail if there is already connected peer connection
    inputs.get_mut_with(&input_ref, |input| {
        input.maybe_replace_session(WhipInputSession {
            peer_connection,
            session_id: session_id.clone(),
        })
    })?;

    if let Some(peer_connection) = weak_pear_connection.upgrade() {
        let input_ref = input_ref.clone();
        let buffer = RtpJitterBufferSharedContext::new(
            &state.ctx,
            RtpJitterBufferMode::RealTime,
            state.ctx.queue_ctx.sync_point,
        );

        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: true,
            audio: true,
            offset: QueueTrackOffset::Pts(Duration::ZERO),
        });

        let video_sender = Arc::new(Mutex::new(video_sender));
        let audio_sender = Arc::new(Mutex::new(audio_sender));

        {
            let video_sender = video_sender.clone();
            let audio_sender = audio_sender.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                video_sender.lock().unwrap().take();
                audio_sender.lock().unwrap().take();
            });
        }

        peer_connection.on_track(move |track_ctx| {
            let ctx = WhipTrackContext::new(track_ctx, &state, &buffer);
            handle_on_track(
                ctx,
                input_ref.clone(),
                video_preferences.clone(),
                &video_sender,
                &audio_sender,
            );
        })
    };

    Ok((session_id, answer))
}
