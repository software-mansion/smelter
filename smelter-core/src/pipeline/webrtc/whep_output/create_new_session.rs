use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::{task::JoinHandle, time::sleep};
use tracing::{debug, warn};
use uuid::Uuid;
use webrtc::peer_connection::{
    peer_connection_state::RTCPeerConnectionState, sdp::session_description::RTCSessionDescription,
};

use crate::pipeline::webrtc::{
    WhipWhepServerState,
    error::WhipWhepServerError,
    handle_keyframe_requests::handle_keyframe_requests,
    whep_output::{
        init_payloaders::{init_audio_payloader, init_video_payloader},
        output::WhepOutputStatsSender,
        peer_connection::PeerConnection,
        stream_media_to_peer::{MediaStream, MediaStreamTask},
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

    let pc = PeerConnection::new(&state.ctx, &video_encoder, &audio_encoder, &offer).await?;

    let (video_stream, video_sender) = match (&video_encoder, video_receiver) {
        (Some(encoder), Some(receiver)) => {
            let (track, sender, ssrc) = pc.new_video_track(encoder).await?;
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

    let (audio_stream, audio_sender) = match (&audio_encoder, audio_receiver) {
        (Some(encoder), Some(receiver)) => {
            let (track, sender, ssrc) = pc.new_audio_track(encoder).await?;
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

    let should_close = register_state_change_handler(&state, &pc, &output_ref, &session_id);

    let sdp_answer = pc
        .negotiate_connection(offer, video_sender.clone(), audio_sender.clone())
        .await?;
    debug!("SDP answer: {}", sdp_answer.sdp);

    if let (Some(sender), Some(keyframe_request_sender)) = (video_sender, keyframe_request_sender) {
        handle_keyframe_requests(&state.ctx.clone(), sender, keyframe_request_sender);
    }

    state.outputs.add_session(&output_ref, &session_id, pc)?;

    MediaStreamTask::new(video_stream, audio_stream, should_close).spawn();

    Ok((session_id, sdp_answer))
}

fn register_state_change_handler(
    server_state: &WhipWhepServerState,
    pc: &PeerConnection,
    output_ref: &Ref<OutputId>,
    session_id: &Arc<str>,
) -> Arc<AtomicBool> {
    let session_id = session_id.clone();
    let stats_sender =
        WhepOutputStatsSender::new(server_state.ctx.stats_sender.clone(), output_ref.clone());

    let cleanup_task_handle: Arc<Mutex<Option<JoinHandle<()>>>> = Default::default();
    let should_close = Arc::new(AtomicBool::new(false));
    let close_flag = should_close.clone();
    let weak_pc = pc.downgrade();

    let cleanup_session = {
        let session_id = session_id.clone();
        let outputs = server_state.outputs.clone();
        let output_ref = output_ref.clone();
        move || {
            if let Err(err) = outputs.remove_session(&output_ref, &session_id) {
                warn!(?session_id, output_id=?output_ref.id(), "Failed to remove session: {err}");
            }
            close_flag.store(true, Ordering::Relaxed);
        }
    };

    pc.on_connection_state_change(move |state| {
        stats_sender.peer_state_changed(&session_id, state);

        match state {
            RTCPeerConnectionState::Connected => {
                if let Ok(mut handle) = cleanup_task_handle.lock()
                    && let Some(task) = handle.take()
                {
                    task.abort();
                }
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected => {
                if let Ok(handle @ None) = cleanup_task_handle.clone().lock().as_deref_mut() {
                    // schedule task only if none is pending, crucial in transitions failed <-> disconnected
                    let cleanup_session = cleanup_session.clone();
                    let weak_pc = weak_pc.clone();
                    let task = tokio::spawn(async move {
                        sleep(Duration::from_secs(60)).await;

                        let Some(pc) = weak_pc.upgrade() else {
                            cleanup_session();
                            return;
                        };

                        // double check if after timeout state it is still disconnected
                        let not_connected = [
                            RTCPeerConnectionState::Unspecified,
                            RTCPeerConnectionState::Failed,
                            RTCPeerConnectionState::Disconnected,
                        ]
                        .contains(&pc.connection_state());
                        if not_connected {
                            cleanup_session()
                        }
                    });
                    *handle = Some(task);
                }
            }
            _ => {
                // Other states aren't crucial for cleanup
            }
        }
    });

    should_close
}
