use tokio::sync::oneshot;

use std::sync::Arc;
use tracing::error;

use whip_input::WhipInputState;

pub mod bearer_token;
pub mod error;
mod init_peer_connection;
pub mod supported_video_codec_parameters;

mod server;
mod whip_input;

pub(super) use server::WhipWhepServer;

use crate::pipeline::PipelineCtx;

#[derive(Debug, Clone)]
struct WhipWhepServerState {
    inputs: WhipInputState,
    ctx: Arc<PipelineCtx>,
}

#[derive(Debug, Default)]
pub struct WhipWhepPipelineState {
    pub inputs: WhipInputState,
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
