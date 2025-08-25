use std::sync::Arc;

use crossbeam_channel::bounded;

use crate::{
    pipeline::{
        input::Input,
        webrtc::{
            bearer_token::generate_token,
            whip_input::connection_state::WhipInputConnectionStateOptions,
        },
    },
    queue::QueueDataReceiver,
};

use crate::prelude::*;

pub(super) mod connection_state;
pub(super) mod negotiated_codecs;
pub(super) mod state;
mod utils;

pub(super) mod track_audio_thread;
pub(super) mod track_video_thread;

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
        state.inputs.add_input(
            &endpoint_id,
            WhipInputConnectionStateOptions {
                bearer_token: bearer_token.clone(),
                video_preferences: options.video_preferences,
                frame_sender,
                input_samples_sender,
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

struct AsyncReceiverIter<T> {
    pub receiver: tokio::sync::mpsc::Receiver<T>,
}

impl<T> Iterator for AsyncReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.blocking_recv()
    }
}
