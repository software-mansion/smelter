use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use compositor_render::{Frame, InputId, OutputId};
use crossbeam_channel::Sender;
use tracing::{info, warn};

use crate::{
    audio_mixer::OutputSamples,
    error::{OutputInitError, RegisterOutputError},
    queue::PipelineEvent,
};

use super::{output, OutputAudioOptions, OutputVideoOptions, Pipeline, PipelineCtx, PipelineInput};

#[derive(Debug, Clone)]
pub enum PipelineOutputEndCondition {
    AnyOf(Vec<InputId>),
    AllOf(Vec<InputId>),
    AnyInput,
    AllInputs,
    Never,
}

pub struct PipelineOutput {
    pub(crate) output: Box<dyn output::Output>,
    pub video_end_condition: Option<PipelineOutputEndConditionState>,
    pub audio_end_condition: Option<PipelineOutputEndConditionState>,
}

pub(super) enum OutputSender<T> {
    ActiveSender(T),
    FinishedSender,
}

pub(super) fn register_pipeline_output<BuildFn, NewOutputResult>(
    pipeline: &Arc<Mutex<Pipeline>>,
    output_id: OutputId,
    video: Option<OutputVideoOptions>,
    audio: Option<OutputAudioOptions>,
    build_output: BuildFn,
) -> Result<NewOutputResult, RegisterOutputError>
where
    BuildFn: FnOnce(
        Arc<PipelineCtx>,
        OutputId,
    ) -> Result<(Box<dyn output::Output>, NewOutputResult), OutputInitError>,
{
    let (has_video, has_audio) = (video.is_some(), audio.is_some());
    if !has_video && !has_audio {
        return Err(RegisterOutputError::NoVideoAndAudio(output_id));
    }

    if pipeline.lock().unwrap().outputs.contains_key(&output_id) {
        return Err(RegisterOutputError::AlreadyRegistered(output_id));
    }

    let pipeline_ctx = pipeline.lock().unwrap().ctx.clone();

    let (output, output_result) = build_output(pipeline_ctx, output_id.clone())
        .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

    let mut guard = pipeline.lock().unwrap();

    if guard.outputs.contains_key(&output_id) {
        return Err(RegisterOutputError::AlreadyRegistered(output_id));
    }

    let output = PipelineOutput {
        output,
        audio_end_condition: audio.as_ref().map(|audio| {
            PipelineOutputEndConditionState::new_audio(audio.end_condition.clone(), &guard.inputs)
        }),
        video_end_condition: video.as_ref().map(|video| {
            PipelineOutputEndConditionState::new_video(video.end_condition.clone(), &guard.inputs)
        }),
    };

    if let (Some(video_opts), Some(video_output)) = (video.clone(), output.output.video()) {
        let result = guard.renderer.update_scene(
            output_id.clone(),
            video_output.resolution,
            video_output.frame_format,
            video_opts.initial,
        );

        if let Err(err) = result {
            guard.renderer.unregister_output(&output_id);
            return Err(RegisterOutputError::SceneError(output_id.clone(), err));
        }
    };

    if let Some(audio_opts) = audio.clone() {
        guard.audio_mixer.register_output(
            output_id.clone(),
            audio_opts.initial,
            audio_opts.mixing_strategy,
            audio_opts.channels,
        );
    }

    guard.outputs.insert(output_id.clone(), output);

    Ok(output_result)
}

impl Pipeline {
    pub(super) fn all_output_video_senders_iter(
        pipeline: &Arc<Mutex<Pipeline>>,
    ) -> impl Iterator<Item = (OutputId, OutputSender<Sender<PipelineEvent<Frame>>>)> {
        let outputs: HashMap<_, _> = pipeline
            .lock()
            .unwrap()
            .outputs
            .iter_mut()
            .filter_map(|(output_id, output)| {
                let eos_status = output.video_end_condition.as_mut()?.eos_status();
                let sender = output.output.video()?.frame_sender.clone();
                Some((output_id.clone(), (sender, eos_status)))
            })
            .collect();

        outputs
            .into_iter()
            .filter_map(|(output_id, (sender, eos_status))| match eos_status {
                EosStatus::None => Some((output_id, OutputSender::ActiveSender(sender))),
                EosStatus::SendEos => {
                    info!(?output_id, "Sending video EOS on output.");
                    if sender.send(PipelineEvent::EOS).is_err() {
                        warn!(
                            ?output_id,
                            "Failed to send EOS from renderer. Channel closed."
                        );
                    };
                    Some((output_id, OutputSender::FinishedSender))
                }
                EosStatus::AlreadySent => None,
            })
    }

    pub(super) fn all_output_audio_senders_iter(
        pipeline: &Arc<Mutex<Pipeline>>,
    ) -> impl Iterator<Item = (OutputId, OutputSender<Sender<PipelineEvent<OutputSamples>>>)> {
        let outputs: HashMap<_, _> = pipeline
            .lock()
            .unwrap()
            .outputs
            .iter_mut()
            .filter_map(|(output_id, output)| {
                let eos_status = output.audio_end_condition.as_mut()?.eos_status();
                let sender = output.output.audio()?.samples_batch_sender.clone();
                Some((output_id.clone(), (sender, eos_status)))
            })
            .collect();

        outputs
            .into_iter()
            .filter_map(|(output_id, (sender, eos_status))| match eos_status {
                EosStatus::None => Some((output_id, OutputSender::ActiveSender(sender))),
                EosStatus::SendEos => {
                    info!(?output_id, "Sending audio EOS on output.");
                    if sender.send(PipelineEvent::EOS).is_err() {
                        warn!(?output_id, "Failed to send EOS from mixer. Channel closed.");
                    };
                    Some((output_id, OutputSender::FinishedSender))
                }
                EosStatus::AlreadySent => None,
            })
    }
}

#[derive(Debug, Clone)]
pub struct PipelineOutputEndConditionState {
    condition: PipelineOutputEndCondition,
    connected_inputs: HashSet<InputId>,
    did_end: bool,
    did_send_eos: bool,
}

enum StateChange<'a> {
    AddInput(&'a InputId),
    RemoveInput(&'a InputId),
    NoChanges,
}

enum EosStatus {
    None,
    SendEos,
    AlreadySent,
}

impl PipelineOutputEndConditionState {
    fn new_video(
        condition: PipelineOutputEndCondition,
        inputs: &HashMap<InputId, PipelineInput>,
    ) -> Self {
        Self {
            condition,
            connected_inputs: inputs
                .iter()
                .filter_map(|(input_id, input)| match input.video_eos_received {
                    Some(false) => Some(input_id.clone()),
                    _ => None,
                })
                .collect(),
            did_end: false,
            did_send_eos: false,
        }
    }

    fn new_audio(
        condition: PipelineOutputEndCondition,
        inputs: &HashMap<InputId, PipelineInput>,
    ) -> Self {
        Self {
            condition,
            connected_inputs: inputs
                .iter()
                .filter_map(|(input_id, input)| match input.audio_eos_received {
                    Some(false) => Some(input_id.clone()),
                    _ => None,
                })
                .collect(),
            did_end: false,
            did_send_eos: false,
        }
    }

    fn eos_status(&mut self) -> EosStatus {
        self.on_event(StateChange::NoChanges);
        if self.did_end {
            if !self.did_send_eos {
                self.did_send_eos = true;
                return EosStatus::SendEos;
            }
            return EosStatus::AlreadySent;
        }
        EosStatus::None
    }

    pub(super) fn did_output_end(&self) -> bool {
        self.did_end
    }

    pub(super) fn on_input_registered(&mut self, input_id: &InputId) {
        self.on_event(StateChange::AddInput(input_id))
    }
    pub(super) fn on_input_unregistered(&mut self, input_id: &InputId) {
        self.on_event(StateChange::RemoveInput(input_id))
    }
    pub(super) fn on_input_eos(&mut self, input_id: &InputId) {
        self.on_event(StateChange::RemoveInput(input_id))
    }

    fn on_event(&mut self, action: StateChange) {
        if self.did_end {
            return;
        }
        match action {
            StateChange::AddInput(id) => {
                self.connected_inputs.insert(id.clone());
            }
            StateChange::RemoveInput(id) => {
                self.connected_inputs.remove(id);
            }
            StateChange::NoChanges => (),
        };
        self.did_end = match self.condition {
            PipelineOutputEndCondition::AnyOf(ref inputs) => inputs
                .iter()
                .any(|input_id| !self.connected_inputs.contains(input_id)),
            PipelineOutputEndCondition::AllOf(ref inputs) => inputs
                .iter()
                .all(|input_id| !self.connected_inputs.contains(input_id)),
            PipelineOutputEndCondition::AnyInput => matches!(action, StateChange::RemoveInput(_)),
            PipelineOutputEndCondition::AllInputs => self.connected_inputs.is_empty(),
            PipelineOutputEndCondition::Never => false,
        };
    }
}
