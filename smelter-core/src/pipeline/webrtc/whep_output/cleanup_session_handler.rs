use std::sync::Arc;

use smelter_render::OutputId;
use tracing::warn;

use crate::pipeline::webrtc::whep_output::state::WhepOutputsState;

#[derive(Clone, Debug)]
pub(crate) struct OnCleanupSessionHdlr {
    outputs: WhepOutputsState,
    output_id: OutputId,
    session_id: Arc<str>,
}

impl OnCleanupSessionHdlr {
    pub fn new(outputs: &WhepOutputsState, output_id: &OutputId, session_id: &Arc<str>) -> Self {
        Self {
            outputs: outputs.clone(),
            output_id: output_id.clone(),
            session_id: session_id.clone(),
        }
    }

    pub async fn call_handler(&self) {
        let Self {
            outputs,
            output_id,
            session_id,
        } = self;
        if let Err(err) = outputs.remove_session(output_id, session_id).await {
            warn!(?session_id, ?output_id, "Failed to remove session: {err}");
        }
    }
}
