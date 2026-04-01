use std::sync::Arc;

use crate::{
    pipeline::{
        input::Input,
        webrtc::{
            WhipInputsState,
            bearer_token::generate_token,
            whip_input::{
                state::WhipInputStateOptions, video_preferences::resolve_video_preferences,
            },
        },
    },
    queue::QueueInput,
};

use crate::prelude::*;

pub(crate) struct WhipInput {
    whip_inputs_state: WhipInputsState,
    input_ref: Ref<InputId>,
}

impl WhipInput {
    pub(crate) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: WhipInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let Some(state) = &ctx.whip_whep_state else {
            return Err(WebrtcServerError::ServerNotRunning.into());
        };
        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Whip,
        });

        let queue_input = QueueInput::new(&ctx, &input_ref, options.required);

        let endpoint_id = options
            .endpoint_override
            .unwrap_or(input_ref.id().0.clone());
        let endpoint_route = Arc::from(format!("/whip/{}", urlencoding::encode(&endpoint_id)));

        let bearer_token = options.bearer_token.unwrap_or_else(generate_token);

        let video_preferences = resolve_video_preferences(&ctx, options.video_preferences)?;

        state.inputs.add_input(
            &input_ref,
            WhipInputStateOptions {
                bearer_token: bearer_token.clone(),
                endpoint_id,
                video_preferences,
                queue_input: queue_input.downgrade(),
                jitter_buffer_options: options.jitter_buffer,
            },
        )?;

        Ok((
            Input::Whip(Self {
                whip_inputs_state: state.inputs.clone(),
                input_ref,
            }),
            InputInitInfo::Whip {
                bearer_token,
                endpoint_route,
            },
            queue_input,
        ))
    }
}

impl Drop for WhipInput {
    fn drop(&mut self) {
        self.whip_inputs_state.remove_input(&self.input_ref);
    }
}
