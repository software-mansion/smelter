use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use smelter_render::OutputId;
use tokio::{task::JoinHandle, time::sleep};
use tracing::warn;
use webrtc::{
    peer_connection::RTCPeerConnection,
    peer_connection::peer_connection_state::RTCPeerConnectionState,
};

use crate::pipeline::webrtc::whep_output::{WhepOutputStatsSender, state::WhepOutputsState};

use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct ConnectionStateChangeHdlr {
    outputs: WhepOutputsState,
    output_ref: Ref<OutputId>,
    session_id: Arc<str>,
    stats_sender: WhepOutputStatsSender,
    cleanup_task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl ConnectionStateChangeHdlr {
    pub fn new(
        ctx: &Arc<PipelineCtx>,
        output_ref: &Ref<OutputId>,
        session_id: &Arc<str>,
        outputs: &WhepOutputsState,
    ) -> Self {
        Self {
            outputs: outputs.clone(),
            output_ref: output_ref.clone(),
            session_id: session_id.clone(),
            stats_sender: WhepOutputStatsSender::new(ctx.stats_sender.clone(), output_ref.clone()),
            cleanup_task_handle: Default::default(),
        }
    }

    pub fn on_state_change(&self, pc: &Arc<RTCPeerConnection>, state: RTCPeerConnectionState) {
        self.stats_sender
            .peer_state_changed(&self.session_id, state);
        self.clone().handle_cleanup_on_disconnect(pc.clone(), state);
    }

    async fn cleanup_session(&self) {
        let ConnectionStateChangeHdlr {
            outputs,
            output_ref,
            session_id,
            stats_sender,
            ..
        } = self;
        if let Err(err) = outputs.remove_session(output_ref, session_id).await {
            warn!(?session_id, output_id=?output_ref.id(), "Failed to remove session: {err}");
        }
        stats_sender.peer_state_changed(session_id, RTCPeerConnectionState::Closed);
    }

    fn handle_cleanup_on_disconnect(
        self,
        pc: Arc<RTCPeerConnection>,
        state: RTCPeerConnectionState,
    ) {
        match state {
            RTCPeerConnectionState::Connected => {
                if let Ok(mut handle) = self.cleanup_task_handle.lock()
                    && let Some(task) = handle.take()
                {
                    task.abort();
                }
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected => {
                if let Ok(handle @ None) = self.cleanup_task_handle.clone().lock().as_deref_mut() {
                    // schedule task only if none is pending, crucial in transitions failed <-> disconnected
                    let task = tokio::spawn(async move {
                        sleep(Duration::from_secs(150)).await; // 2 min 30 s

                        let current_state = pc.connection_state();
                        if current_state != RTCPeerConnectionState::Connected
                            && current_state != RTCPeerConnectionState::Connecting
                            && current_state != RTCPeerConnectionState::Closed
                        {
                            self.cleanup_session().await;
                        }
                    });
                    *handle = Some(task);
                }
            }
            _ => {
                // Other states aren't crucial for cleanup
            }
        }
    }
}
