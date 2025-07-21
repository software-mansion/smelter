use std::sync::{Arc, Mutex};

use compositor_render::InputId;

use crate::{
    error::{InputInitError, RegisterInputError},
    pipeline::{decoder::DecodedDataReceiver, input::Input, PipelineCtx},
    queue::QueueInputOptions,
    Pipeline,
};

use super::input;

pub struct PipelineInput {
    pub input: input::Input,

    /// Some(received) - Whether EOS was received from queue on audio stream for that input.
    /// None - No audio configured for that input.
    pub(super) audio_eos_received: Option<bool>,
    /// Some(received) - Whether EOS was received from queue on video stream for that input.
    /// None - No video configured for that input.
    pub(super) video_eos_received: Option<bool>,
}

/// This method doesn't take pipeline lock for the whole scope,
/// because input registration can potentially take a relatively long time.
pub(super) fn register_pipeline_input<BuildFn, NewInputResult>(
    pipeline: &Arc<Mutex<Pipeline>>,
    input_id: InputId,
    queue_options: QueueInputOptions,
    build_input: BuildFn,
) -> Result<NewInputResult, RegisterInputError>
where
    BuildFn: FnOnce(
        Arc<PipelineCtx>,
        InputId,
    ) -> Result<(Input, NewInputResult, DecodedDataReceiver), InputInitError>,
{
    if pipeline.lock().unwrap().inputs.contains_key(&input_id) {
        return Err(RegisterInputError::AlreadyRegistered(input_id));
    }

    let pipeline_ctx = pipeline.lock().unwrap().ctx().clone();

    let (input, input_result, receiver) = build_input(pipeline_ctx, input_id.clone())
        .map_err(|err| RegisterInputError::InputError(input_id.clone(), err))?;

    let (audio_eos_received, video_eos_received) = (
        receiver.audio.as_ref().map(|_| false),
        receiver.video.as_ref().map(|_| false),
    );

    let pipeline_input = PipelineInput {
        input,
        audio_eos_received,
        video_eos_received,
    };

    let mut guard = pipeline.lock().unwrap();

    if guard.inputs.contains_key(&input_id) {
        return Err(RegisterInputError::AlreadyRegistered(input_id));
    };

    if pipeline_input.audio_eos_received.is_some() {
        for (_, output) in guard.outputs.iter_mut() {
            if let Some(ref mut cond) = output.audio_end_condition {
                cond.on_input_registered(&input_id);
            }
        }
    }

    if pipeline_input.video_eos_received.is_some() {
        for (_, output) in guard.outputs.iter_mut() {
            if let Some(ref mut cond) = output.video_end_condition {
                cond.on_input_registered(&input_id);
            }
        }
    }

    guard.inputs.insert(input_id.clone(), pipeline_input);
    guard.queue.add_input(&input_id, receiver, queue_options);
    guard.renderer.register_input(input_id);

    Ok(input_result)
}

impl PipelineInput {
    pub(super) fn on_audio_eos(&mut self) {
        self.audio_eos_received = self.audio_eos_received.map(|_| true);
    }
    pub(super) fn on_video_eos(&mut self) {
        self.audio_eos_received = self.audio_eos_received.map(|_| true);
    }
}
