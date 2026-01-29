use crate::{pipeline::rtmp::rtmp_input::state::RtmpInputsState, prelude::*};
use std::sync::Arc;

mod input;
mod on_connection;
mod process_audio;
mod process_video;
mod stream_state;

pub(crate) mod state;
pub use input::RtmpServerInput;
pub(crate) use on_connection::handle_on_connection;

#[derive(Clone)]
struct RtmpConnectionContext {
    ctx: Arc<PipelineCtx>,
    inputs: RtmpInputsState,
    input_ref: Ref<InputId>,
    app: Arc<str>,
    stream_key: Arc<str>,
}

impl RtmpConnectionContext {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        inputs: RtmpInputsState,
        input_ref: Ref<InputId>,
        app: Arc<str>,
        stream_key: Arc<str>,
    ) -> Self {
        Self {
            ctx,
            inputs,
            input_ref,
            app,
            stream_key,
        }
    }
}
