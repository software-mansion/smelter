use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use crossbeam_channel::Sender;
use smelter_render::{OutputFrameFormat, error::ErrorStack};
use tracing::{error, info, warn};

use crate::pipeline::{
    hls::HlsOutput,
    input::PipelineInput,
    moq::MoqClientOutput,
    mp4::Mp4Output,
    rtmp::RtmpClientOutput,
    rtp::RtpOutput,
    webrtc::{WhepOutput, WhipOutput},
};
use crate::prelude::*;

pub(crate) struct PipelineOutput {
    pub output: Box<dyn Output>,
    /// Unique per registration, unlike `OutputId` which can be reused after unregister.
    pub output_ref: Ref<OutputId>,
    pub state: OutputState,
}

pub(crate) enum OutputState {
    /// Output was registered with `start_at` and did not reach that timestamp yet. It is
    /// already created (e.g. connection with a server is established), but it is not
    /// connected to the renderer/audio mixer.
    NotStarted {
        video: Option<RegisterOutputVideoOptions>,
        audio: Option<RegisterOutputAudioOptions>,
    },
    /// Output is connected to the renderer/audio mixer and receives frames/samples.
    Started {
        video_end_condition: Option<PipelineOutputEndConditionState>,
        audio_end_condition: Option<PipelineOutputEndConditionState>,
    },
}

impl PipelineOutput {
    /// End conditions are set for exactly those tracks the output was registered with,
    /// so they double as the "has video"/"has audio" flag after the start.
    pub(super) fn has_video(&self) -> bool {
        match &self.state {
            OutputState::NotStarted { video, .. } => video.is_some(),
            OutputState::Started {
                video_end_condition,
                ..
            } => video_end_condition.is_some(),
        }
    }

    pub(super) fn has_audio(&self) -> bool {
        match &self.state {
            OutputState::NotStarted { audio, .. } => audio.is_some(),
            OutputState::Started {
                audio_end_condition,
                ..
            } => audio_end_condition.is_some(),
        }
    }

    /// `None` until the output starts.
    pub(super) fn video_end_condition(&self) -> Option<&PipelineOutputEndConditionState> {
        match &self.state {
            OutputState::NotStarted { .. } => None,
            OutputState::Started {
                video_end_condition,
                ..
            } => video_end_condition.as_ref(),
        }
    }

    pub(super) fn video_end_condition_mut(
        &mut self,
    ) -> Option<&mut PipelineOutputEndConditionState> {
        match &mut self.state {
            OutputState::NotStarted { .. } => None,
            OutputState::Started {
                video_end_condition,
                ..
            } => video_end_condition.as_mut(),
        }
    }

    /// `None` until the output starts.
    pub(super) fn audio_end_condition(&self) -> Option<&PipelineOutputEndConditionState> {
        match &self.state {
            OutputState::NotStarted { .. } => None,
            OutputState::Started {
                audio_end_condition,
                ..
            } => audio_end_condition.as_ref(),
        }
    }

    pub(super) fn audio_end_condition_mut(
        &mut self,
    ) -> Option<&mut PipelineOutputEndConditionState> {
        match &mut self.state {
            OutputState::NotStarted { .. } => None,
            OutputState::Started {
                audio_end_condition,
                ..
            } => audio_end_condition.as_mut(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputVideo<'a> {
    pub resolution: Resolution,
    pub frame_format: OutputFrameFormat,
    pub frame_sender: &'a Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: &'a Sender<()>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputAudio<'a> {
    pub samples_batch_sender: &'a Sender<PipelineEvent<OutputAudioSamples>>,
}

pub(crate) trait Output: Send {
    fn audio(&self) -> Option<OutputAudio<'_>>;
    fn video(&self) -> Option<OutputVideo<'_>>;
    fn kind(&self) -> OutputProtocolKind;
}

pub(super) fn new_external_output(
    ctx: Arc<PipelineCtx>,
    output_ref: Ref<OutputId>,
    options: ProtocolOutputOptions,
) -> Result<(Box<dyn Output>, Option<Port>), OutputInitError> {
    match options {
        ProtocolOutputOptions::Rtp(opt) => {
            let (output, port) = RtpOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), Some(port)))
        }
        ProtocolOutputOptions::Rtmp(opt) => {
            let output = RtmpClientOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::Mp4(opt) => {
            let output = Mp4Output::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::Hls(opt) => {
            let output = HlsOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::Whip(opt) => {
            let output = WhipOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::Whep(opt) => {
            let output = WhepOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::MoqClient(opt) => {
            let output = MoqClientOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), None))
        }
    }
}

pub(super) enum OutputSender<T> {
    ActiveSender(T),
    FinishedSender,
}

/// Creates the output. If `start_at` is set, then output is only created, and it is connected
/// to the renderer/audio mixer after the queue reaches that timestamp. Otherwise, it is
/// connected immediately.
pub(super) fn register_pipeline_output<BuildFn, NewOutputResult>(
    pipeline: &Arc<Mutex<Pipeline>>,
    output_id: OutputId,
    video: Option<RegisterOutputVideoOptions>,
    audio: Option<RegisterOutputAudioOptions>,
    start_at: Option<Duration>,
    build_output: BuildFn,
) -> Result<NewOutputResult, RegisterOutputError>
where
    BuildFn: FnOnce(
        Arc<PipelineCtx>,
        Ref<OutputId>,
    ) -> Result<(Box<dyn Output>, NewOutputResult), OutputInitError>,
{
    if video.is_none() && audio.is_none() {
        return Err(RegisterOutputError::NoVideoAndAudio(output_id));
    }

    let pipeline_ctx = {
        // Do not hold the pipeline lock for the whole scope, because output creation
        // can potentially take a relatively long time.
        let guard = pipeline.lock().unwrap();
        if guard.outputs.contains_key(&output_id) {
            return Err(RegisterOutputError::AlreadyRegistered(output_id));
        }
        guard.ctx.clone()
    };

    let output_ref = Ref::new(&output_id);
    let (output, output_result) = build_output(pipeline_ctx, output_ref.clone())
        .map_err(|err| RegisterOutputError::OutputError(output_id.clone(), err))?;

    let mut guard = pipeline.lock().unwrap();
    if guard.outputs.contains_key(&output_id) {
        return Err(RegisterOutputError::AlreadyRegistered(output_id));
    }

    guard.outputs.insert(
        output_id.clone(),
        PipelineOutput {
            output,
            output_ref: output_ref.clone(),
            state: OutputState::NotStarted { video, audio },
        },
    );

    let Some(start_at) = start_at else {
        // Start while still holding the lock, so errors (e.g. invalid scene) are reported
        // by this call instead of asynchronously.
        if let Err(err) = guard.start_registered_output(&output_ref) {
            guard.outputs.remove(&output_id);
            return Err(err);
        }
        return Ok(output_result);
    };

    // Do not hold the pipeline lock across `Pipeline::schedule_event(...)`.
    drop(guard);
    Pipeline::schedule_event(
        pipeline,
        start_at,
        LateEventPolicy::AlwaysRun,
        move |pipeline| {
            if let Err(err) = pipeline.start_registered_output(&output_ref) {
                pipeline.outputs.remove(output_ref.id());
                error!(
                    "Error while starting output scheduled for pts {}ms: {}",
                    start_at.as_millis(),
                    ErrorStack::new(&err).into_string()
                )
            }
        },
    );

    Ok(output_result)
}

impl Pipeline {
    /// Connects already created output to the renderer/audio mixer, from that point
    /// output starts receiving frames/samples.
    fn start_registered_output(
        &mut self,
        output_ref: &Ref<OutputId>,
    ) -> Result<(), RegisterOutputError> {
        let inputs = &self.inputs;
        let output_id = output_ref.id();
        let Some(output) = self.outputs.get_mut(output_id) else {
            // output was unregistered before it started
            return Ok(());
        };
        if &output.output_ref != output_ref {
            // different output with the same id
            return Ok(());
        }

        let OutputState::NotStarted { video, audio } = &mut output.state else {
            warn!(output_id=%output_ref, "Output already started");
            return Ok(());
        };
        let (video, audio) = (video.take(), audio.take());

        output.state = OutputState::Started {
            video_end_condition: video.as_ref().map(|video| {
                PipelineOutputEndConditionState::new_video(video.end_condition.clone(), inputs)
            }),
            audio_end_condition: audio.as_ref().map(|audio| {
                PipelineOutputEndConditionState::new_audio(audio.end_condition.clone(), inputs)
            }),
        };

        if let (Some(video_opts), Some(video_output)) = (video, output.output.video()) {
            let result = self.renderer.update_scene(
                output_id.clone(),
                video_output.resolution,
                video_output.frame_format,
                video_opts.initial,
            );

            if let Err(err) = result {
                self.renderer.unregister_output(output_id);
                return Err(RegisterOutputError::SceneError(output_id.clone(), err));
            }
        };

        if let Some(audio_opts) = audio {
            self.audio_mixer.register_output(
                output_id.clone(),
                audio_opts.initial,
                audio_opts.mixing_strategy,
                audio_opts.channels,
            );
        }

        Ok(())
    }

    pub(super) fn all_output_video_senders_iter(
        pipeline: &Arc<Mutex<Pipeline>>,
    ) -> impl Iterator<Item = (OutputId, OutputSender<Sender<PipelineEvent<Frame>>>)> {
        let outputs: HashMap<_, _> = pipeline
            .lock()
            .unwrap()
            .outputs
            .iter_mut()
            .filter_map(|(output_id, output)| {
                let eos_status = output.video_end_condition_mut()?.eos_status();
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
    ) -> impl Iterator<
        Item = (
            OutputId,
            OutputSender<Sender<PipelineEvent<OutputAudioSamples>>>,
        ),
    > {
        let outputs: HashMap<_, _> = pipeline
            .lock()
            .unwrap()
            .outputs
            .iter_mut()
            .filter_map(|(output_id, output)| {
                let eos_status = output.audio_end_condition_mut()?.eos_status();
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
