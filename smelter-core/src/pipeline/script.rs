use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crossbeam_channel::Receiver;
use smelter_render::{
    Framerate, OutputFrameFormat, Renderer,
    error::{ErrorStack, UpdateSceneError},
    scene::{Component, ComponentSize, Scene, SceneError},
};
use tracing::{debug, warn};

use crate::{
    audio_mixer::AudioMixer,
    event::{Event, EventEmitter},
    queue::QueueVideoFrame,
};

use crate::prelude::*;

/// Errors are logged and sent to clients. A script can fail on every frame, so only the first
/// `MAX_REPORTED_ERRORS` errors in every `ERROR_REPORT_INTERVAL` are reported.
const ERROR_REPORT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_REPORTED_ERRORS: u32 = 5;

/// Scripts that drive output scenes.
///
/// Scripts run on the renderer thread before each frame, so nothing here waits for the pipeline
/// lock. Lock order is: pipeline, `OutputScripts`, renderer/audio mixer.
#[derive(Clone)]
pub(crate) struct OutputScripts(Arc<Mutex<OutputScriptsState>>);

struct OutputScriptsState {
    runtime: Option<Arc<dyn ScriptRuntime>>,
    renderer: Renderer,
    audio_mixer: AudioMixer,
    event_emitter: Arc<EventEmitter>,
    pipeline_events: Receiver<Event>,
    framerate: Framerate,

    scripts: HashMap<OutputId, OutputScript>,
    inputs: InputTracker,
    /// PTS of the last frame passed to scripts, relative to the pipeline start.
    last_pts: Timestamp,
}

struct OutputScript {
    source: Arc<str>,
    script: Box<dyn Script>,
    target: ScriptTarget,
    /// Last scene produced by the script that was accepted by the renderer.
    video: Option<Component>,
    /// Last audio config produced by the script.
    audio: Option<AudioMixerConfig>,
    /// Start of the current `ERROR_REPORT_INTERVAL` and number of errors since then.
    reported_errors: (Instant, u32),
}

/// Where the result of a script is applied.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScriptTarget {
    pub resolution: Resolution,
    pub frame_format: OutputFrameFormat,
    pub has_audio: bool,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SetSceneError {
    #[error(transparent)]
    Script(#[from] ScriptError),

    #[error(transparent)]
    Scene(#[from] UpdateSceneError),
}

impl OutputScripts {
    pub fn new(
        runtime: Option<Arc<dyn ScriptRuntime>>,
        renderer: Renderer,
        audio_mixer: AudioMixer,
        event_emitter: Arc<EventEmitter>,
        framerate: Framerate,
    ) -> Self {
        Self(Arc::new(Mutex::new(OutputScriptsState {
            runtime,
            renderer,
            audio_mixer,
            pipeline_events: event_emitter.subscribe(),
            event_emitter,
            framerate,
            scripts: HashMap::new(),
            inputs: InputTracker::default(),
            last_pts: Timestamp::ZERO,
        })))
    }

    /// Sets what is rendered for the output.
    ///
    /// - A static scene is passed to the renderer and the script of that output is stopped.
    /// - A script is started and the scene it produces is passed to the renderer. If the
    ///   output already runs a script with the same source, only its props are replaced.
    ///
    /// If a static scene or a new script is rejected, the output keeps its current scene and
    /// script. An error returned for an update of props comes from the running script, props
    /// are replaced anyway.
    pub fn set_scene(
        &self,
        output_id: &OutputId,
        scene: OutputScene,
        target: ScriptTarget,
    ) -> Result<(), SetSceneError> {
        let mut guard = self.0.lock().unwrap();
        match scene {
            OutputScene::Static(scene) => {
                guard.renderer.update_scene(
                    output_id.clone(),
                    target.resolution,
                    target.frame_format,
                    scene,
                )?;
                guard.scripts.remove(output_id);
                Ok(())
            }
            OutputScene::Script(spec) => guard.set_script(output_id, spec, target),
        }
    }

    pub fn remove(&self, output_id: &OutputId) {
        self.0.lock().unwrap().scripts.remove(output_id);
    }

    /// Whether the audio config of that output is defined by its script.
    pub fn controls_audio(&self, output_id: &OutputId) -> bool {
        let guard = self.0.lock().unwrap();
        let script = guard.scripts.get(output_id);
        script.is_some_and(|script| script.audio.is_some())
    }

    /// Events emitted by the pipeline since the last call. Needs to be called before reading
    /// the list of registered inputs passed to `process_frame`. That way every event refers to
    /// an input from that list, unless the input was already unregistered.
    pub fn take_pipeline_events(&self) -> Vec<Event> {
        self.0.lock().unwrap().pipeline_events.try_iter().collect()
    }

    /// Runs scripts for the frame that is about to be rendered and applies their results.
    ///
    /// - `pts` - relative to the pipeline start.
    pub fn process_frame(
        &self,
        pts: Timestamp,
        pipeline_events: Vec<Event>,
        registered_inputs: Vec<Ref<InputId>>,
        frames: &HashMap<InputId, QueueVideoFrame>,
    ) {
        self.0
            .lock()
            .unwrap()
            .process_frame(pts, pipeline_events, registered_inputs, frames)
    }
}

impl OutputScriptsState {
    fn set_script(
        &mut self,
        output_id: &OutputId,
        spec: ScriptSpec,
        target: ScriptTarget,
    ) -> Result<(), SetSceneError> {
        // State of inputs is updated only when a frame is processed. A script started between
        // frames receives everything that changed since then before the next frame.
        let frame = ScriptFrame {
            pts: self.last_pts,
            inputs: &self.inputs.inputs,
            events: &[],
        };

        if let Some(output) = self.scripts.get_mut(output_id)
            && output.source == spec.source
        {
            debug!(?output_id, "Update props of a scene script");
            output.target = target;
            output.script.set_props(spec.props);
            let mut update = match output.script.update(&frame) {
                Ok(update) => update,
                Err(err) => {
                    if let ScriptError::Terminated(_) = err {
                        self.scripts.remove(output_id);
                    }
                    return Err(err.into());
                }
            };
            let errors = std::mem::take(&mut update.errors);
            output.apply(output_id, update, &mut self.renderer, &self.audio_mixer)?;
            return match errors.into_iter().next() {
                Some(err) => Err(err.into()),
                None => Ok(()),
            };
        }

        let Some(runtime) = &self.runtime else {
            return Err(ScriptError::RuntimeNotAvailable.into());
        };
        let mut script = runtime.load(
            &spec,
            ScriptOutputCtx {
                output_id: output_id.clone(),
                resolution: target.resolution,
                framerate: self.framerate,
                host: Arc::new(RendererScriptHost(self.renderer.clone())),
            },
        )?;
        // Nothing is applied if the script does not work from the start.
        let mut update = script.update(&frame)?;
        if !update.errors.is_empty() {
            return Err(update.errors.remove(0).into());
        }
        if update.video.is_none() {
            return Err(
                ScriptError::InvalidScene("Script did not produce a scene.".to_string()).into(),
            );
        }

        let mut output = OutputScript {
            source: spec.source,
            script,
            target,
            video: None,
            audio: None,
            reported_errors: (Instant::now(), 0),
        };
        output.apply(output_id, update, &mut self.renderer, &self.audio_mixer)?;
        self.scripts.insert(output_id.clone(), output);
        Ok(())
    }

    fn process_frame(
        &mut self,
        pts: Timestamp,
        pipeline_events: Vec<Event>,
        registered_inputs: Vec<Ref<InputId>>,
        frames: &HashMap<InputId, QueueVideoFrame>,
    ) {
        let mut events = Vec::new();
        self.inputs
            .sync_registered_inputs(registered_inputs, &mut events);
        for event in pipeline_events {
            self.inputs.apply_pipeline_event(event, &mut events);
        }
        self.inputs.apply_frames(frames, &mut events);

        // Scripts started from other threads use `last_pts`, it can't go back.
        self.last_pts = self.last_pts.max(pts);

        let frame = ScriptFrame {
            pts: self.last_pts,
            inputs: &self.inputs.inputs,
            events: &events,
        };
        let mut terminated = vec![];
        for (output_id, output) in self.scripts.iter_mut() {
            let mut update = match output.script.update(&frame) {
                Ok(update) => update,
                Err(err) => {
                    if let ScriptError::Terminated(_) = err {
                        terminated.push(output_id.clone());
                    }
                    output.report_error(output_id, err, &self.event_emitter);
                    continue;
                }
            };
            for err in std::mem::take(&mut update.errors) {
                output.report_error(output_id, err, &self.event_emitter);
            }
            let result = output.apply(output_id, update, &mut self.renderer, &self.audio_mixer);
            if let Err(err) = result {
                let err = ScriptError::InvalidScene(ErrorStack::new(&err).into_string());
                output.report_error(output_id, err, &self.event_emitter);
            }
        }

        // The output keeps the last scene. The script can be started again with an update
        // request, even if it has the same source.
        for output_id in terminated {
            self.scripts.remove(&output_id);
        }
    }
}

impl OutputScript {
    /// Passes what the script produced to the renderer and the audio mixer, unless it is the
    /// same as the last time.
    fn apply(
        &mut self,
        output_id: &OutputId,
        update: ScriptUpdate,
        renderer: &mut Renderer,
        audio_mixer: &AudioMixer,
    ) -> Result<(), UpdateSceneError> {
        if let Some(video) = update.video
            && self.video.as_ref() != Some(&video)
        {
            debug!(?output_id, "Update scene from a script {:?}", video);
            renderer.update_scene(
                output_id.clone(),
                self.target.resolution,
                self.target.frame_format,
                Scene::ComponentTree(video.clone()),
            )?;
            self.video = Some(video);
        }

        if let Some(audio) = update.audio
            && self.target.has_audio
            && self.audio.as_ref() != Some(&audio)
        {
            debug!(?output_id, "Update audio mixer from a script {:?}", audio);
            audio_mixer.update_output(output_id, audio.clone())?;
            self.audio = Some(audio);
        }

        Ok(())
    }

    fn report_error(&mut self, output_id: &OutputId, err: ScriptError, emitter: &EventEmitter) {
        let is_terminated = matches!(err, ScriptError::Terminated(_));

        let (interval_start, count) = &mut self.reported_errors;
        if interval_start.elapsed() >= ERROR_REPORT_INTERVAL {
            *interval_start = Instant::now();
            *count = 0;
        }
        *count += 1;
        // Termination happens once and it needs to be visible.
        if *count > MAX_REPORTED_ERRORS && !is_terminated {
            if *count == MAX_REPORTED_ERRORS + 1 {
                warn!(
                    ?output_id,
                    "Scene script reports too many errors, ignoring some of them."
                );
            }
            return;
        }

        warn!(?output_id, "Scene script error: {err}");
        let severity = match is_terminated {
            true => ErrorSeverity::Critical,
            false => ErrorSeverity::Transient,
        };
        emitter.emit(Event::OutputError {
            output_id: output_id.clone(),
            severity,
            err: err.into(),
        });
    }
}

struct RendererScriptHost(Renderer);

impl ScriptHost for RendererScriptHost {
    fn measure_component(&self, component: Component) -> Result<ComponentSize, SceneError> {
        self.0.measure_component(component)
    }
}

/// State of inputs as seen by scripts.
#[derive(Default)]
struct InputTracker {
    inputs: Vec<ScriptInput>,
    /// Registration of each element of `inputs`. An input registered again with the same id is
    /// a new input.
    refs: Vec<Ref<InputId>>,
    /// Video EOS that was delivered along with the last frame, for each element of `inputs`. It
    /// is reported one frame later, so the script still renders the last frame.
    pending_video_eos: Vec<bool>,
}

impl InputTracker {
    fn sync_registered_inputs(
        &mut self,
        mut registered: Vec<Ref<InputId>>,
        events: &mut Vec<ScriptEvent>,
    ) {
        let mut index = 0;
        while index < self.refs.len() {
            if registered.contains(&self.refs[index]) {
                index += 1;
                continue;
            }
            self.refs.remove(index);
            self.pending_video_eos.remove(index);
            let input = self.inputs.remove(index);
            events.push(ScriptEvent {
                input_id: input.input_id,
                kind: ScriptEventKind::InputUnregistered,
            });
        }

        registered.retain(|input_ref| !self.refs.contains(input_ref));
        registered.sort_by_key(|input_ref| input_ref.creation_order());
        for input_ref in registered {
            events.push(ScriptEvent {
                input_id: input_ref.id().clone(),
                kind: ScriptEventKind::InputRegistered,
            });
            self.inputs.push(ScriptInput {
                input_id: input_ref.id().clone(),
                video: None,
                audio: None,
                resolution: None,
            });
            self.refs.push(input_ref);
            self.pending_video_eos.push(false);
        }
    }

    fn apply_pipeline_event(&mut self, event: Event, events: &mut Vec<ScriptEvent>) {
        use ScriptEventKind as Kind;
        use TrackState::*;

        let (input_id, kind) = match event {
            Event::VideoInputStreamDelivered(id) => (id, Kind::VideoDelivered),
            Event::VideoInputStreamPlaying(id) => (id, Kind::VideoPlaying),
            Event::VideoInputStreamPaused(id) => (id, Kind::VideoPaused),
            Event::AudioInputStreamDelivered(id) => (id, Kind::AudioDelivered),
            Event::AudioInputStreamPlaying(id) => (id, Kind::AudioPlaying),
            Event::AudioInputStreamPaused(id) => (id, Kind::AudioPaused),
            Event::AudioInputStreamEos(id) => (id, Kind::AudioEos),
            // Reported based on frames, see `apply_frames`.
            Event::VideoInputStreamEos(_) => return,
            Event::OutputDone(_) | Event::OutputError { .. } => return,
        };
        let Some(input) = self.inputs.iter_mut().find(|i| i.input_id == input_id) else {
            return;
        };
        match kind {
            Kind::VideoDelivered if matches!(input.video, None | Some(Finished)) => {
                input.video = Some(Ready)
            }
            // Start of the playback is reported based on frames, but resume is not visible
            // there, because a paused input keeps producing frames.
            Kind::VideoPlaying if input.video == Some(Paused) => input.video = Some(Playing),
            Kind::VideoPaused => input.video = Some(Paused),
            Kind::AudioDelivered if matches!(input.audio, None | Some(Finished)) => {
                input.audio = Some(Ready)
            }
            Kind::AudioPlaying => input.audio = Some(Playing),
            Kind::AudioPaused => input.audio = Some(Paused),
            Kind::AudioEos => input.audio = Some(Finished),
            _ => return,
        }
        events.push(ScriptEvent { input_id, kind });
    }

    /// Updates video state based on the frames that are about to be rendered.
    fn apply_frames(
        &mut self,
        frames: &HashMap<InputId, QueueVideoFrame>,
        events: &mut Vec<ScriptEvent>,
    ) {
        use ScriptEventKind as Kind;

        for (input, pending_eos) in self.inputs.iter_mut().zip(&mut self.pending_video_eos) {
            let input_id = input.input_id.clone();
            let mut emit = |kind| {
                events.push(ScriptEvent {
                    input_id: input_id.clone(),
                    kind,
                })
            };

            if std::mem::take(pending_eos) {
                input.video = Some(TrackState::Finished);
                emit(Kind::VideoEos);
            }

            let Some(queue_frame) = frames.get(&input.input_id) else {
                continue;
            };
            if let Some(frame) = &queue_frame.frame {
                let is_resized = input.resolution != Some(frame.resolution);
                input.resolution = Some(frame.resolution);
                if !matches!(input.video, Some(TrackState::Playing | TrackState::Paused)) {
                    input.video = Some(TrackState::Playing);
                    emit(Kind::VideoPlaying);
                } else if is_resized {
                    emit(Kind::VideoResized);
                }
            }
            if queue_frame.is_eos {
                match queue_frame.frame {
                    Some(_) => *pending_eos = true,
                    None => {
                        input.video = Some(TrackState::Finished);
                        emit(Kind::VideoEos);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use smelter_render::{FrameData, YuvPlanes};

    use super::*;

    use ScriptEventKind as Kind;

    fn frame(width: usize) -> Option<Frame> {
        Some(Frame {
            data: FrameData::PlanarYuv420(YuvPlanes {
                y_plane: Bytes::new(),
                u_plane: Bytes::new(),
                v_plane: Bytes::new(),
            }),
            resolution: Resolution { width, height: 360 },
            pts: Timestamp::ZERO,
        })
    }

    struct TestTracker {
        tracker: InputTracker,
        registered: Vec<Ref<InputId>>,
    }

    impl TestTracker {
        fn new(input_ids: &[&str]) -> Self {
            let mut tracker = Self {
                tracker: InputTracker::default(),
                registered: vec![],
            };
            for input_id in input_ids {
                tracker.register(input_id);
            }
            tracker
        }

        fn register(&mut self, input_id: &str) {
            self.registered.push(Ref::new(&InputId(input_id.into())));
        }

        fn unregister(&mut self, input_id: &str) {
            self.registered.retain(|r| &*r.id().0 != input_id);
        }

        /// Processes a frame, returns events as (input id, kind).
        fn process(
            &mut self,
            pipeline_events: Vec<Event>,
            frames: Vec<(&str, Option<Frame>, bool)>,
        ) -> Vec<(String, Kind)> {
            let frames = frames
                .into_iter()
                .map(|(id, frame, is_eos)| (InputId(id.into()), QueueVideoFrame { frame, is_eos }))
                .collect();
            let mut events = vec![];
            // Order of the list is not defined, pipeline keeps inputs in a hash map.
            let registered = self.registered.iter().rev().cloned().collect();
            self.tracker.sync_registered_inputs(registered, &mut events);
            for event in pipeline_events {
                self.tracker.apply_pipeline_event(event, &mut events);
            }
            self.tracker.apply_frames(&frames, &mut events);
            events
                .into_iter()
                .map(|event| (event.input_id.0.to_string(), event.kind))
                .collect()
        }

        fn input(&self, input_id: &str) -> &ScriptInput {
            let inputs = &self.tracker.inputs;
            inputs.iter().find(|i| &*i.input_id.0 == input_id).unwrap()
        }

        fn input_ids(&self) -> Vec<&str> {
            let inputs = &self.tracker.inputs;
            inputs.iter().map(|i| &*i.input_id.0).collect()
        }
    }

    fn event(kind: Kind, input_id: &str) -> (String, Kind) {
        (input_id.to_string(), kind)
    }

    #[test]
    fn inputs_are_listed_in_registration_order() {
        let mut tracker = TestTracker::new(&["b", "a"]);
        assert_eq!(
            tracker.process(vec![], vec![]),
            [
                event(Kind::InputRegistered, "b"),
                event(Kind::InputRegistered, "a")
            ]
        );

        tracker.register("c");
        tracker.unregister("b");
        assert_eq!(
            tracker.process(vec![], vec![]),
            [
                event(Kind::InputUnregistered, "b"),
                event(Kind::InputRegistered, "c")
            ]
        );
        assert_eq!(tracker.input_ids(), ["a", "c"]);
    }

    #[test]
    fn input_registered_again_is_a_new_input() {
        let mut tracker = TestTracker::new(&["a"]);
        tracker.process(vec![], vec![("a", frame(640), false)]);
        assert_eq!(tracker.input("a").video, Some(TrackState::Playing));

        tracker.unregister("a");
        tracker.register("a");
        assert_eq!(
            tracker.process(vec![], vec![]),
            [
                event(Kind::InputUnregistered, "a"),
                event(Kind::InputRegistered, "a")
            ]
        );
        assert_eq!(tracker.input("a").video, None);
        assert_eq!(tracker.input("a").resolution, None);
    }

    #[test]
    fn video_state_follows_frames() {
        let mut tracker = TestTracker::new(&["a"]);
        tracker.process(vec![], vec![]);

        // Events from the pipeline are ahead of frames. Start of playback and EOS are ignored,
        // because they are reported when frames get here.
        let id = InputId("a".into());
        let events = tracker.process(
            vec![
                Event::VideoInputStreamDelivered(id.clone()),
                Event::VideoInputStreamPlaying(id.clone()),
                Event::VideoInputStreamEos(id.clone()),
            ],
            vec![("a", None, false)],
        );
        assert_eq!(events, [event(Kind::VideoDelivered, "a")]);
        assert_eq!(tracker.input("a").video, Some(TrackState::Ready));

        let events = tracker.process(vec![], vec![("a", frame(640), false)]);
        assert_eq!(events, [event(Kind::VideoPlaying, "a")]);
        assert_eq!(tracker.input("a").resolution.unwrap().width, 640);

        assert_eq!(tracker.process(vec![], vec![("a", frame(640), false)]), []);

        let events = tracker.process(vec![], vec![("a", frame(1280), false)]);
        assert_eq!(events, [event(Kind::VideoResized, "a")]);

        // EOS without a frame, nothing is rendered for this input anymore.
        let events = tracker.process(vec![], vec![("a", None, true)]);
        assert_eq!(events, [event(Kind::VideoEos, "a")]);
        assert_eq!(tracker.input("a").video, Some(TrackState::Finished));

        // Next track of the same input.
        let events = tracker.process(vec![], vec![("a", frame(1280), false)]);
        assert_eq!(events, [event(Kind::VideoPlaying, "a")]);
    }

    #[test]
    fn eos_with_last_frame_is_reported_on_next_frame() {
        let mut tracker = TestTracker::new(&["a"]);
        tracker.process(vec![], vec![("a", frame(640), false)]);

        // The last frame still needs to be rendered with the input in the scene.
        assert_eq!(tracker.process(vec![], vec![("a", frame(640), true)]), []);
        assert_eq!(tracker.input("a").video, Some(TrackState::Playing));

        let events = tracker.process(vec![], vec![("a", None, false)]);
        assert_eq!(events, [event(Kind::VideoEos, "a")]);

        // The next track starts right after the previous one.
        tracker.process(vec![], vec![("a", frame(640), false)]);
        tracker.process(vec![], vec![("a", frame(640), true)]);
        let events = tracker.process(vec![], vec![("a", frame(640), false)]);
        assert_eq!(
            events,
            [event(Kind::VideoEos, "a"), event(Kind::VideoPlaying, "a")]
        );
        assert_eq!(tracker.input("a").video, Some(TrackState::Playing));
    }

    #[test]
    fn paused_input_keeps_state_while_frames_are_rendered() {
        let mut tracker = TestTracker::new(&["a"]);
        let id = InputId("a".into());
        tracker.process(vec![], vec![("a", frame(640), false)]);

        let events = tracker.process(
            vec![Event::VideoInputStreamPaused(id.clone())],
            vec![("a", frame(640), false)],
        );
        assert_eq!(events, [event(Kind::VideoPaused, "a")]);
        assert_eq!(tracker.process(vec![], vec![("a", frame(640), false)]), []);
        assert_eq!(tracker.input("a").video, Some(TrackState::Paused));

        let events = tracker.process(
            vec![Event::VideoInputStreamPlaying(id)],
            vec![("a", frame(640), false)],
        );
        assert_eq!(events, [event(Kind::VideoPlaying, "a")]);
    }

    #[test]
    fn resolution_of_input_paused_before_first_frame_is_reported() {
        let mut tracker = TestTracker::new(&["a"]);
        let id = InputId("a".into());
        tracker.process(vec![Event::VideoInputStreamPaused(id)], vec![]);

        // Every change of the input needs an event, scripts do not see it otherwise.
        let events = tracker.process(vec![], vec![("a", frame(640), false)]);
        assert_eq!(events, [event(Kind::VideoResized, "a")]);
        assert_eq!(tracker.input("a").video, Some(TrackState::Paused));
        assert_eq!(tracker.input("a").resolution.unwrap().width, 640);
    }

    #[test]
    fn audio_state_follows_pipeline_events() {
        let mut tracker = TestTracker::new(&["a"]);
        let id = InputId("a".into());
        let events = tracker.process(
            vec![
                Event::AudioInputStreamDelivered(id.clone()),
                Event::AudioInputStreamPlaying(id.clone()),
                // unknown input
                Event::AudioInputStreamPlaying(InputId("b".into())),
            ],
            vec![],
        );
        assert_eq!(
            events,
            [
                event(Kind::InputRegistered, "a"),
                event(Kind::AudioDelivered, "a"),
                event(Kind::AudioPlaying, "a"),
            ]
        );

        let events = tracker.process(vec![Event::AudioInputStreamEos(id)], vec![]);
        assert_eq!(events, [event(Kind::AudioEos, "a")]);
        assert_eq!(tracker.input("a").audio, Some(TrackState::Finished));
        assert_eq!(tracker.input("a").video, None);
    }
}
