use tokio::sync::oneshot;

use std::sync::Arc;
use tracing::error;

use whip_input::WhipInputsState;

mod bearer_token;
mod error;
mod peer_connection_recvonly;
mod server;
mod supported_video_codec_parameters;
mod whip_input;
mod whip_output;

pub(super) use server::WhipWhepServer;
pub(super) use whip_input::WhipInput;
pub(super) use whip_output::WhipOutput;

use crate::pipeline::PipelineCtx;

#[derive(Debug, Clone)]
struct WhipWhepServerState {
    inputs: WhipInputsState,
    ctx: Arc<PipelineCtx>,
}

#[derive(Debug)]
pub struct WhipWhepPipelineState {
    pub port: u16,
    pub inputs: WhipInputsState,
}

impl WhipWhepPipelineState {
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(Self {
            port,
            inputs: WhipInputsState::default(),
        })
    }
}

#[derive(Debug)]
pub struct WhipWhepServerHandle {
    shutdown_sender: Option<oneshot::Sender<()>>,
}

impl Drop for WhipWhepServerHandle {
    fn drop(&mut self) {
        if let Some(sender) = self.shutdown_sender.take() {
            if sender.send(()).is_err() {
                error!("Cannot send shutdown signal to WHIP WHEP server")
            }
        }
    }
}
