use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing::{debug, warn};
use uuid::Uuid;
use webrtc::{
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::RTPCodecType,
};

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::{
        rtp::{RtpJitterBufferMode, RtpJitterBufferSharedContext},
        webrtc::{
            WhipWhepServerState,
            error::WhipWhepServerError,
            offer_codec_filter::codecs_from_offer,
            peer_connection_recvonly::RecvonlyPeerConnection,
            whip_input::{
                WhipTrackContext,
                on_track::{handle_audio_track, handle_video_track},
                state::WhipInputSession,
                video_preferences::video_params_compliant_with_offer,
            },
        },
    },
    queue::{QueueInput, QueueTrackOffset, QueueTrackOptions},
};

use crate::prelude::*;

/// Tracks which media tracks have arrived and defers queue creation
/// until we know exactly which track types the WHIP client sends.
struct DeferredTrackState {
    queue_input: QueueInput,
    resolved: bool,
    video_track: Option<(WhipTrackContext, Ref<InputId>, Vec<VideoDecoderOptions>)>,
    audio_track: Option<(WhipTrackContext, Ref<InputId>)>,
}

impl DeferredTrackState {
    /// Resolve the deferred state: create the queue track with only the arrived
    /// track types and start processing all buffered tracks.
    fn resolve(&mut self) {
        if self.resolved {
            return;
        }
        self.resolved = true;

        let has_video = self.video_track.is_some();
        let has_audio = self.audio_track.is_some();

        if !has_video && !has_audio {
            warn!("Deferred track state resolved with no tracks");
            return;
        }

        let (video_sender, audio_sender) = self.queue_input.queue_new_track(QueueTrackOptions {
            video: has_video,
            audio: has_audio,
            offset: QueueTrackOffset::Pts(Duration::ZERO),
        });

        if let Some((ctx, input_ref, video_preferences)) = self.video_track.take()
            && let Some(sender) = video_sender
        {
            handle_video_track(ctx, input_ref, video_preferences, sender);
        }

        if let Some((ctx, input_ref)) = self.audio_track.take()
            && let Some(sender) = audio_sender
        {
            handle_audio_track(ctx, input_ref, sender);
        }
    }
}

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

        let deferred_state = Arc::new(Mutex::new(DeferredTrackState {
            queue_input,
            resolved: false,
            video_track: None,
            audio_track: None,
        }));

        // Timeout: if the second track hasn't arrived within 3 seconds,
        // resolve with whatever tracks we have so far. This prevents
        // indefinite waiting when a client only sends audio or only video.
        {
            let deferred_state = deferred_state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                deferred_state.lock().unwrap().resolve();
            });
        }

        peer_connection.on_track(move |track_ctx| {
            let ctx = WhipTrackContext::new(track_ctx, &state, &buffer);
            let kind = ctx.track.kind();
            debug!(?kind, input_id=%input_ref, "on_track called");

            let mut deferred = deferred_state.lock().unwrap();
            match kind {
                RTPCodecType::Video => {
                    if deferred.video_track.is_some() {
                        warn!("Video track already registered");
                        return;
                    }
                    deferred.video_track =
                        Some((ctx, input_ref.clone(), video_preferences.clone()));
                }
                RTPCodecType::Audio => {
                    if deferred.audio_track.is_some() {
                        warn!("Audio track already registered");
                        return;
                    }
                    deferred.audio_track = Some((ctx, input_ref.clone()));
                }
                RTPCodecType::Unspecified => {
                    warn!("Unknown track kind");
                    return;
                }
            }

            // If both tracks have arrived, resolve immediately without
            // waiting for the timeout.
            if deferred.video_track.is_some() && deferred.audio_track.is_some() {
                deferred.resolve();
            }
        })
    };

    Ok((session_id, answer))
}
