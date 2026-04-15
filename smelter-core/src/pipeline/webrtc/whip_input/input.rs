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

/// WHIP input - receives WebRTC ingest via WHIP HTTP endpoint, decodes, and feeds
/// frames/samples into the queue.
///
/// ## Codec negotiation
///
/// Remote client sends SDP offer. We extract codecs from the offer and echo all
/// offered variants in our answer for codec types matching our decoder preferences.
/// For Vulkan H.264, the offer codecs are further filtered by hardware decode
/// capabilities (unsupported profiles/levels are dropped).
///
/// ## Timestamps
///
/// - On connection
///   - PTS of first frame will be synced to `queue_sync_point` Instant
///   - Register track with `QueueTrackOffset::Pts(Duration::ZERO)`
///   - Jitter buffer: `RtpJitterBufferMode::RealTime` produces timestamps already in the correct
///     time frame
///
/// ### Unsupported scenarios
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
/// - If other input is required and delays queue by X relative to `queue_sync_point.elapsed()`:
///   - If X is smaller than channel sizes then, this input latency will
///     be artificially increased by X.
///   - If X is larger than channel size then, this input will be intermittently
///     blank and streaming until the other inputs (and queue processing) catch up.
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
