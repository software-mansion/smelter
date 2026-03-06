use std::sync::Arc;

use smelter_render::OutputId;
use tracing::warn;

use crate::pipeline::webrtc::whep_output::state::WhepOutputsState;

use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct OnCleanupSessionHdlr {
    outputs: WhepOutputsState,
    output_ref: Ref<OutputId>,
    session_id: Arc<str>,
    stats_sender: StatsSender,
}

impl OnCleanupSessionHdlr {
    pub fn new(
        outputs: &WhepOutputsState,
        output_ref: &Ref<OutputId>,
        session_id: &Arc<str>,
        stats_sender: &StatsSender,
    ) -> Self {
        Self {
            outputs: outputs.clone(),
            output_ref: output_ref.clone(),
            session_id: session_id.clone(),
            stats_sender: stats_sender.clone(),
        }
    }

    pub async fn call_handler(&self) {
        let Self {
            outputs,
            output_ref,
            session_id,
            stats_sender,
        } = self;
        if let Err(err) = outputs
            .remove_session(output_ref, session_id, stats_sender)
            .await
        {
            warn!(?session_id, output_id=?output_ref.id(), "Failed to remove session: {err}");
        }
    }
}
