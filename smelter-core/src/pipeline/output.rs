use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
    time::Instant,
};

use crossbeam_channel::{SendTimeoutError, Sender};
use smelter_render::{OutputFrameFormat, error::ErrorStack};
use tracing::{debug, error, warn};

use crate::{
    pipeline::{
        hls::HlsOutput,
        moq::MoqClientOutput,
        mp4::Mp4Output,
        rtmp::RtmpClientOutput,
        rtp::RtpOutput,
        script::{ScriptTarget, SetSceneError},
        webrtc::{WhepOutput, WhipOutput},
    },
    stats::OutputStatsGuard,
};

use crate::prelude::*;

/// Capacity of the channel between the audio mixer and an output. Around 1 second, because
/// audio gaps are more noticeable than dropped frames and batches are cheap to buffer.
pub(crate) const AUDIO_TRACK_CHANNEL_SIZE: usize = 50;

#[derive(Default)]
pub(crate) struct PipelineOutputs(HashMap<OutputId, PipelineOutput>);

impl Deref for PipelineOutputs {
    type Target = HashMap<OutputId, PipelineOutput>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PipelineOutputs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PipelineOutputs {
    pub(super) fn video_senders(&self) -> HashMap<OutputId, Sender<PipelineEvent<Frame>>> {
        self.iter()
            .filter_map(|(id, output)| Some((id.clone(), output.output.video()?)))
            .map(|(id, video)| (id, video.frame_sender.clone()))
            .collect()
    }

    pub(super) fn audio_senders(
        &self,
    ) -> HashMap<OutputId, Sender<PipelineEvent<OutputAudioSamples>>> {
        self.iter()
            .filter_map(|(id, output)| Some((id.clone(), output.output.audio()?)))
            .map(|(id, audio)| (id, audio.samples_batch_sender.clone()))
            .collect()
    }
}

pub(crate) struct PipelineOutput {
    pub output: Box<dyn Output>,
    /// Unique per registration, unlike `OutputId` which can be reused after unregister.
    pub output_ref: Ref<OutputId>,
    pub state: OutputState,
    _stats_guard: OutputStatsGuard,
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum OutputState {
    /// Output was registered with `start_at` and did not reach that timestamp yet. It is
    /// already created (e.g. connection with a server is established), but it is not
    /// connected to the renderer/audio mixer.
    NotStarted {
        video: Option<RegisterOutputVideoOptions>,
        audio: Option<RegisterOutputAudioOptions>,
    },
    /// Output is connected to the renderer/audio mixer and receives frames/samples.
    Started { has_video: bool, has_audio: bool },
}

impl PipelineOutput {
    /// Whether the output was registered with a video track.
    pub(super) fn has_video(&self) -> bool {
        match &self.state {
            OutputState::NotStarted { video, .. } => video.is_some(),
            OutputState::Started { has_video, .. } => *has_video,
        }
    }

    /// Whether the output was registered with an audio track.
    pub(super) fn has_audio(&self) -> bool {
        match &self.state {
            OutputState::NotStarted { audio, .. } => audio.is_some(),
            OutputState::Started { has_audio, .. } => *has_audio,
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
) -> Result<(Box<dyn Output>, OutputInitInfo), OutputInitError> {
    match options {
        ProtocolOutputOptions::Rtp(opt) => {
            let (output, port) = RtpOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), OutputInitInfo::Rtp { port }))
        }
        ProtocolOutputOptions::Rtmp(opt) => {
            let output = RtmpClientOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), OutputInitInfo::Other))
        }
        ProtocolOutputOptions::Mp4(opt) => {
            let output = Mp4Output::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), OutputInitInfo::Other))
        }
        ProtocolOutputOptions::Hls(opt) => {
            let output = HlsOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), OutputInitInfo::Other))
        }
        ProtocolOutputOptions::Whip(opt) => {
            let output = WhipOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), OutputInitInfo::Other))
        }
        ProtocolOutputOptions::Whep(opt) => {
            let (output, endpoint_route) = WhepOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), OutputInitInfo::Whep { endpoint_route }))
        }
        ProtocolOutputOptions::MoqClient(opt) => {
            let output = MoqClientOutput::new(ctx, output_ref, opt)?;
            Ok((Box::new(output), OutputInitInfo::Other))
        }
    }
}

/// Decides how long the renderer and audio mixer threads can wait on a single output, so one
/// output that stopped consuming does not delay the others.
#[derive(Debug, Clone, Copy)]
pub(super) struct OutputSendPolicy {
    pub sync_point: Instant,
    pub never_drop: bool,
}

/// The receiving side of an output track is gone.
pub(super) struct OutputTrackClosed;

impl OutputSendPolicy {
    /// Waits at most until `pts` is due and then drops the data for this output only. Live
    /// processing only handles data that is already due, so that is a single attempt.
    pub(super) fn send<T>(
        &self,
        sender: &Sender<PipelineEvent<T>>,
        data: T,
        pts: Timestamp,
    ) -> Result<(), OutputTrackClosed> {
        let event = PipelineEvent::Data(data);
        if self.never_drop {
            return sender.send(event).map_err(|_| OutputTrackClosed);
        }
        match sender.send_deadline(event, self.sync_point + pts) {
            Ok(()) => Ok(()),
            Err(SendTimeoutError::Timeout(_)) => {
                debug!(?pts, "Output is not consuming data. Dropping it.");
                Ok(())
            }
            Err(SendTimeoutError::Disconnected(_)) => Err(OutputTrackClosed),
        }
    }
}

/// Creates the output. If `start_at` is set, then output is only created, and it is connected
/// to the renderer/audio mixer after the queue reaches that timestamp. Otherwise, it is
/// connected immediately.
pub(super) fn register_pipeline_output<BuildFn, NewOutputResult>(
    pipeline: &Arc<Mutex<Pipeline>>,
    output_id: OutputId,
    kind: OutputProtocolKind,
    video: Option<RegisterOutputVideoOptions>,
    audio: Option<RegisterOutputAudioOptions>,
    start_at: Option<Timestamp>,
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
    // Registered before the build, so events sent while building are not dropped. Any error
    // below, including removing the output after a failed start, drops the guard, which
    // removes the output from stats.
    let stats_guard =
        OutputStatsGuard::new(output_ref.clone(), kind, pipeline_ctx.stats_sender.clone());
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
            _stats_guard: stats_guard,
        },
    );

    let Some(start_at) = start_at else {
        // Start while still holding the lock, so errors (e.g. invalid scene) are reported
        // by this call instead of asynchronously.
        if let Err(err) = start_registered_output(&mut guard, &output_ref) {
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
            if let Err(err) = start_registered_output(pipeline, &output_ref) {
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

/// Connects already created output to the renderer/audio mixer, from that point
/// output starts receiving frames/samples.
fn start_registered_output(
    pipeline: &mut Pipeline,
    output_ref: &Ref<OutputId>,
) -> Result<(), RegisterOutputError> {
    let output_id = output_ref.id();
    let Some(output) = pipeline.outputs.get_mut(output_id) else {
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
        has_video: video.is_some(),
        has_audio: audio.is_some(),
    };

    // Audio is registered first, because a scene script can replace the audio config.
    let has_audio = audio.is_some();
    if let Some(audio_opts) = audio {
        pipeline.audio_mixer.register_output(
            output_id.clone(),
            audio_opts.initial,
            audio_opts.mixing_strategy,
            audio_opts.channels,
        );
    }

    if let (Some(video_opts), Some(video_output)) = (video, output.output.video()) {
        let target = ScriptTarget {
            resolution: video_output.resolution,
            frame_format: video_output.frame_format,
            has_audio,
        };
        let result = pipeline
            .scripts
            .set_scene(output_id, video_opts.initial, target);

        if let Err(err) = result {
            pipeline.audio_mixer.unregister_output(output_id);
            pipeline.renderer.unregister_output(output_id);
            return Err(match err {
                SetSceneError::Scene(err) => {
                    RegisterOutputError::SceneError(output_id.clone(), err)
                }
                SetSceneError::Script(err) => {
                    RegisterOutputError::ScriptError(output_id.clone(), err)
                }
            });
        }
    };

    Ok(())
}
