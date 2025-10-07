use tokio::sync::oneshot;

use std::sync::Arc;
use tracing::{error, info};

use whip_input::WhipInputsState;

mod bearer_token;
mod error;
mod handle_keyframe_requests;
mod negotiated_codecs;
mod peer_connection_recvonly;
mod server;
mod supported_video_codec_parameters;
mod trickle_ice_utils;
mod whep_output;
mod whip_input;
mod whip_output;

pub(super) use server::WhipWhepServer;
pub(super) use whep_output::WhepOutput;
pub(super) use whip_input::WhipInput;
pub(super) use whip_output::WhipOutput;

use crate::pipeline::{PipelineCtx, webrtc::whep_output::state::WhepOutputsState};

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
    shutdown_sender: Option<oneshot::Sender<()>>,
}

impl Drop for WhipWhepServerHandle {
    fn drop(&mut self) {
        info!("Stopping WHIP/WHEP server");
        if let Some(sender) = self.shutdown_sender.take()
            && sender.send(()).is_err()
        {
            error!("Cannot send shutdown signal to WHIP/WHEP server")
        }
    }
}
