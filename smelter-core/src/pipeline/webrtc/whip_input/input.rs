use std::sync::Arc;

use crossbeam_channel::bounded;

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
    queue::QueueDataReceiver,
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
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let Some(state) = &ctx.whip_whep_state else {
            return Err(WebrtcServerError::ServerNotRunning.into());
        };
        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Whip,
        });

        let endpoint_id = options
            .endpoint_override
            .unwrap_or(input_ref.id().0.clone());
        let (frame_sender, frame_receiver) = bounded(5);
        let (input_samples_sender, input_samples_receiver) = bounded(5);

        let bearer_token = options.bearer_token.unwrap_or_else(generate_token);

        let video_preferences = resolve_video_preferences(&ctx, options.video_preferences)?;

        state.inputs.add_input(
            &input_ref,
            WhipInputStateOptions {
                bearer_token: bearer_token.clone(),
                endpoint_id,
                video_preferences,
                frame_sender,
                input_samples_sender,
                jitter_buffer_options: options.jitter_buffer,
            },
        )?;

        Ok((
            Input::Whip(Self {
                whip_inputs_state: state.inputs.clone(),
                input_ref,
            }),
            InputInitInfo::Whip { bearer_token },
            QueueDataReceiver {
                video: Some(frame_receiver),
                audio: Some(input_samples_receiver),
            },
        ))
    }
}

impl Drop for WhipInput {
    fn drop(&mut self) {
        self.whip_inputs_state.ensure_input_closed(&self.input_ref);
    }
}
