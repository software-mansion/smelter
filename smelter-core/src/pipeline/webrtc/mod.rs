use std::sync::Arc;

use tokio::{runtime::Handle, sync::oneshot, task::JoinHandle};
use tracing::{error, info};

mod bearer_token;
mod error;
mod h264_vulkan_capability_filter;
mod handle_keyframe_requests;
mod http_client;
mod input_rtcp_listener;
mod input_rtp_reader;
mod input_thread;
mod negotiated_codecs;
mod offer_codec_filter;
mod peer_connection_recvonly;
mod server;
mod setting_engine;
mod supported_codec_parameters;
mod trickle_ice_utils;

mod whep_input;
mod whep_output;
mod whip_input;
mod whip_output;

pub(super) use server::WhipWhepServer;
pub(super) use setting_engine::WebrtcSettingEngineCtx;
pub(super) use whep_input::WhepInput;
pub(super) use whep_output::WhepOutput;
pub(super) use whip_input::WhipInput;
pub(super) use whip_output::WhipOutput;

use crate::pipeline::{
    PipelineCtx,
    webrtc::{whep_output::state::WhepOutputsState, whip_input::state::WhipInputsState},
};

#[derive(Debug, Clone)]
struct WhipWhepServerState {
    inputs: WhipInputsState,
    outputs: WhepOutputsState,
    ctx: Arc<PipelineCtx>,
}

#[derive(Debug)]
pub struct WhipWhepPipelineState {
    pub port: u16,
    pub inputs: WhipInputsState,
    pub outputs: WhepOutputsState,
}

impl WhipWhepPipelineState {
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(Self {
            port,
            inputs: WhipInputsState::default(),
            outputs: WhepOutputsState::default(),
        })
    }
}

#[derive(Debug)]
pub struct WhipWhepServerHandle {
    pub(super) shutdown_sender: Option<oneshot::Sender<()>>,
    /// Handle to the axum server task. Awaited on Drop so the task — which
    /// holds an `Arc<PipelineCtx>` (and therefore a clone of every
    /// `Arc<vk_video::*>`) — has fully exited before the pipeline tears down.
    pub(super) server_task: Option<JoinHandle<()>>,
    /// Tokio runtime handle, used to `block_on` the join in Drop.
    pub(super) runtime: Handle,
}

impl Drop for WhipWhepServerHandle {
    fn drop(&mut self) {
        info!("Stopping WHIP/WHEP server");
        if let Some(sender) = self.shutdown_sender.take()
            && sender.send(()).is_err()
        {
            error!("Cannot send shutdown signal to WHIP/WHEP server")
        }
        if let Some(handle) = self.server_task.take() {
            // Wait for the WHIP/WHEP server's tokio task to finish so its
            // `Arc<PipelineCtx>` is released before pipeline shutdown
            // continues. We can't `await` here, so block on the runtime.
            if let Err(err) = self.runtime.block_on(handle) {
                error!(?err, "WHIP/WHEP server task panicked during join");
            }
        }
    }
}

pub struct AsyncReceiverIter<T> {
    pub receiver: tokio::sync::mpsc::Receiver<T>,
}

impl<T> Iterator for AsyncReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.blocking_recv()
    }
}
