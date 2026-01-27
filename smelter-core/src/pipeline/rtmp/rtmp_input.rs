use std::sync::Arc;

use crossbeam_channel::bounded;

use crate::{
    pipeline::{
        input::Input,
        rtmp::state::{RtmpInputStateOptions, RtmpInputsState},
    },
    queue::QueueDataReceiver,
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
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let (Some(state), Some(_server)) = (&ctx.rtmp_state, &ctx.rtmp_server) else {
            return Err(RtmpServerError::ServerNotRunning.into());
        };

        let (frame_sender, frame_receiver) = bounded(5);
        let (input_samples_sender, input_samples_receiver) = bounded(5);

        state.inputs.add_input(
            &input_ref,
            RtmpInputStateOptions {
                app: options.app,
                stream_key: options.stream_key,
                frame_sender,
                input_samples_sender,
                video_decoders: options.video_decoders,
                ctx: Arc::downgrade(&ctx),
            },
        )?;

        state.inputs.set_ctx(Arc::downgrade(&ctx));

        Ok((
            Input::RtmpServer(Self {
                rtmp_inputs_state: state.inputs.clone(),
                input_ref,
            }),
            InputInitInfo::Other,
            QueueDataReceiver {
                video: Some(frame_receiver),
                audio: Some(input_samples_receiver),
            },
        ))
    }
}

impl Drop for RtmpServerInput {
    fn drop(&mut self) {
        todo!()
    }
}
