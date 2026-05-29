use std::sync::Arc;

use crate::{
    pipeline::{
        input::Input,
        moq::state::{MoqInputStateOptions, MoqServerInputsState},
    },
    queue::QueueInput,
};

use crate::prelude::*;

pub struct MoqServerInput {
    moq_inputs_state: MoqServerInputsState,
    input_ref: Ref<InputId>,
}

impl MoqServerInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: MoqServerInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let Some(state) = &ctx.moq_state else {
            return Err(MoqServerError::ServerNotRunning.into());
        };

        let queue_input = QueueInput::new(&ctx, &input_ref, options.queue_options);

        state.inputs.add_input(
            &input_ref,
            MoqInputStateOptions {
                broadcast_path: options.broadcast_path,
                queue_input: queue_input.downgrade(),
                decoders: options.decoders,
            },
        )?;

        Ok((
            Input::MoqServer(Self {
                moq_inputs_state: state.inputs.clone(),
                input_ref,
            }),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

impl Drop for MoqServerInput {
    fn drop(&mut self) {
        self.moq_inputs_state.remove_input(&self.input_ref);
    }
}
