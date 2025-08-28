use std::{future::Future, pin::Pin, sync::Arc};

use compositor_render::OutputId;
use tracing::warn;

use crate::pipeline::webrtc::whep_output::state::WhepOutputsState;

pub(crate) fn create_cleanup_session_handler(
    outputs: WhepOutputsState,
    output_id: OutputId,
    session_id: Arc<str>,
) -> impl Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Clone + Sync + 'static {
    move || {
        let outputs = outputs.clone();
        let output_id = output_id.clone();
        let session_id = session_id.clone();

        Box::pin({
            async move {
                if let Err(err) = outputs.remove_session(&output_id, &session_id).await {
                    warn!(?session_id, ?output_id, "Failed to remove session: {err}");
                }
            }
        })
    }
}
