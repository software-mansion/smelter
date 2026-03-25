use std::sync::Arc;

use crate::{
    pipeline::{
        input::Input,
        rtmp::rtmp_input::state::{RtmpInputStateOptions, RtmpInputsState},
    },
    queue::QueueInput,
};

use crate::prelude::*;

/// RTMP server input - waits for an incoming RTMP connection matching a registered
/// app/stream_key pair, demuxes H.264/AAC, decodes, and feeds frames/samples into
/// the queue.
///
/// ## Timestamps
///
/// - On connection:
///   - A new track is created with `QueueTrackOffset::Pts(effective_last_pts + buffer)`.
///     The buffer gives time for data to arrive and be decoded before the queue
///     needs it.
///   - PTS values are normalized to zero (subtracts first observed PTS, shared across
///     video and audio).
/// - On reconnect:
///   - Only one active connection per input is allowed (`ensure_no_active_connection`).
///   - Once a previous connection finishes, a new one can connect and creates a fresh
///     track.
/// - After 5s without receiving both tracks (e.g. audio-only stream), unused track
///   senders are dropped.
///
/// ### Unsupported scenarios
/// - Timestamps are synchronized based on the connection end. If first packet is delivered
///   few seconds after connection the frames will not reach queue on time.
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
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
