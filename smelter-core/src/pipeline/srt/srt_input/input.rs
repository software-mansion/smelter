use std::sync::Arc;

use crate::{
    pipeline::{input::Input, srt::srt_input::state::SrtInputStateOptions},
    queue::QueueInput,
};

use crate::prelude::*;

/// Thin registration shim for an SRT input. The shared SRT server (started by
/// the pipeline) accepts incoming connections and dispatches them by
/// `streamid` to the input registered under that id.
pub struct SrtInput {
    inputs_state: super::super::server::SrtInputsState,
    input_ref: Ref<InputId>,
}

impl SrtInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: SrtInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let Some(state) = &ctx.srt_state else {
            return Err(SrtServerError::ServerNotRunning.into());
        };

        if opts.video.is_none() && opts.audio.is_none() {
            return Err(SrtInputError::NoVideoOrAudio.into());
        }

        if let Some(video) = &opts.video
            && video.decoder == VideoDecoderOptions::VulkanH264
            && !ctx.graphics_context.has_vulkan_decoder_support()
        {
            return Err(InputInitError::DecoderError(
                DecoderInitError::VulkanContextRequiredForVulkanDecoder,
            ));
        }

        let queue_input = QueueInput::new(&ctx, &input_ref, opts.queue_options.clone());

        state.inputs.add_input(
            &input_ref,
            SrtInputStateOptions {
                stream_id: input_ref.id().0.clone(),
                queue_input: queue_input.downgrade(),
                video: opts.video,
                audio: opts.audio,
                queue_options: opts.queue_options,
                offset: opts.offset,
                encryption: opts.encryption,
            },
        )?;

        Ok((
            Input::Srt(Self {
                inputs_state: state.inputs.clone(),
                input_ref,
            }),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

impl Drop for SrtInput {
    fn drop(&mut self) {
        self.inputs_state.remove_input(&self.input_ref);
    }
}
