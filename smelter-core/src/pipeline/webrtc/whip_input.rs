use std::sync::Arc;

use crate::{
    pipeline::{
        input::Input,
        webrtc::{
            bearer_token::generate_token,
            whip_input::{
                connection_state::WhipInputConnectionStateOptions,
                resolve_video_preferences::resolve_video_preferences,
            },
        },
    },
    queue::QueueDataReceiver,
};

use crate::prelude::*;

pub(super) mod connection_state;
pub(super) mod process_tracks;
mod resolve_video_preferences;
pub(super) mod state;

use crossbeam_channel::bounded;
pub(super) use state::WhipInputsState;

pub struct WhipInput {
    whip_inputs_state: WhipInputsState,
    endpoint_id: Arc<str>,
}

impl WhipInput {
    pub(crate) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        options: WhipInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let Some(state) = &ctx.whip_whep_state else {
            return Err(InputInitError::WhipWhepServerNotRunning);
        };

        let endpoint_id = options.endpoint_override.unwrap_or(input_id.0);
        let (frame_sender, frame_receiver) = bounded(5);
        let (input_samples_sender, input_samples_receiver) = bounded(5);

        let bearer_token = options.bearer_token.unwrap_or_else(generate_token);

        let (video_preferences, video_codecs) =
            resolve_video_preferences(&ctx, options.video_preferences)?;

        state.inputs.add_input(
            &endpoint_id,
            WhipInputConnectionStateOptions {
                bearer_token: bearer_token.clone(),
                video_preferences,
                video_codecs,
                frame_sender,
                input_samples_sender,
                buffer_options: options.buffer,
            },
        );

        Ok((
            Input::Whip(Self {
                whip_inputs_state: state.inputs.clone(),
                endpoint_id,
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
        self.whip_inputs_state
            .ensure_input_closed(&self.endpoint_id);
    }
}
