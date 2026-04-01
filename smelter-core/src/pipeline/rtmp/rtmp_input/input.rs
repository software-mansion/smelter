use std::sync::Arc;

use crate::{
    pipeline::{
        input::Input,
        rtmp::rtmp_input::state::{RtmpInputStateOptions, RtmpInputsState},
    },
    queue::QueueInput,
};

use crate::prelude::*;

pub struct RtmpServerInput {
    rtmp_inputs_state: RtmpInputsState,
    input_ref: Ref<InputId>,
}

impl RtmpServerInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: RtmpServerInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let Some(state) = &ctx.rtmp_state else {
            return Err(RtmpServerError::ServerNotRunning.into());
        };

        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Rtmp,
        });

        let queue_input = QueueInput::new(&ctx, &input_ref, options.required);

        state.inputs.add_input(
            &input_ref,
            RtmpInputStateOptions {
                app: options.app,
                stream_key: options.stream_key,
                queue_input: queue_input.downgrade(),
                decoders: options.decoders,
            },
        )?;

        Ok((
            Input::RtmpServer(Self {
                rtmp_inputs_state: state.inputs.clone(),
                input_ref,
            }),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

impl Drop for RtmpServerInput {
    fn drop(&mut self) {
        self.rtmp_inputs_state.remove_input(&self.input_ref);
    }
}
