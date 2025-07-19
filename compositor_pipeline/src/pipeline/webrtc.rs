use tokio::sync::oneshot;

use std::sync::Arc;
use tracing::error;

use whip_input::WhipInputsState;

pub mod bearer_token;
pub mod error;
pub mod supported_video_codec_parameters;

mod peer_connection_recvonly;
mod server;
mod whip_input;

pub(super) use server::WhipWhepServer;
pub(super) use whip_input::WhipInput;

pub use whip_input::WhipInputOptions;

use crate::pipeline::PipelineCtx;

#[derive(Debug, Clone)]
struct WhipWhepServerState {
    inputs: WhipInputsState,
    ctx: Arc<PipelineCtx>,
}

#[derive(Debug, Default)]
pub struct WhipWhepPipelineState {
    pub(crate) inputs: WhipInputsState,
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
