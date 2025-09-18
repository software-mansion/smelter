use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    thread,
    time::{Duration, Instant},
};

use crossbeam_channel::{bounded, Receiver};
use glyphon::fontdb;
use tokio::runtime::Runtime;
use tracing::{error, info, trace, warn};

use smelter_render::{
    error::{
        ErrorStack, RegisterRendererError, RequestKeyframeError, UnregisterRendererError,
        UpdateSceneError,
    },
    scene::Component,
    FrameSet, InputId, OutputId, RegistryType, Renderer, RendererId, RendererOptions, RendererSpec,
};

use crate::{
    audio_mixer::AudioMixer,
    event::{Event, EventEmitter},
    pipeline::{
        channel::{EncodedDataOutput, RawDataInput, RawDataOutput},
        input::{new_external_input, register_pipeline_input, PipelineInput},
        output::{new_external_output, register_pipeline_output, OutputSender, PipelineOutput},
        webrtc::{WhipWhepPipelineState, WhipWhepServer, WhipWhepServerHandle},
    },
    queue::{Queue, QueueAudioOutput, QueueOptions, QueueVideoOutput},
};
use crate::{
    graphics_context::{GraphicsContext, GraphicsContextOptions},
    prelude::*,
};

pub struct Pipeline {
    pub(super) ctx: Arc<PipelineCtx>,
    pub(super) inputs: HashMap<InputId, PipelineInput>,
    pub(super) outputs: HashMap<OutputId, PipelineOutput>,
    pub(super) queue: Arc<Queue>,
    pub(super) renderer: Renderer,
    pub(super) audio_mixer: AudioMixer,
    pub(super) is_started: bool,

    #[allow(dead_code)]
    // triggers cleanup on drop
    whip_whep_handle: Option<WhipWhepServerHandle>,
}

impl Pipeline {
    pub fn new(opts: PipelineOptions) -> Result<Self, InitPipelineError> {
        create_pipeline(opts)
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn ctx(&self) -> &Arc<PipelineCtx> {
        &self.ctx
    }

    pub fn subscribe_pipeline_events(&self) -> Receiver<Event> {
        self.ctx.event_emitter.subscribe()
    }

    pub fn register_input(
        pipeline: &Arc<Mutex<Self>>,
        input_id: InputId,
        options: RegisterInputOptions,
    ) -> Result<InputInitInfo, RegisterInputError> {
        let input_options = options.input_options;
        register_pipeline_input(
            pipeline,
            input_id,
            options.queue_options,
            |ctx, input_id| new_external_input(ctx, input_id, input_options),
        )
    }

    pub fn register_raw_data_input(
        pipeline: &Arc<Mutex<Self>>,
        input_id: InputId,
        raw_input_options: RawDataInputOptions,
        queue_options: QueueInputOptions,
    ) -> Result<RawDataInputSender, RegisterInputError> {
        register_pipeline_input(pipeline, input_id, queue_options, |ctx, input_id| {
            RawDataInput::new_input(ctx, input_id, raw_input_options)
        })
    }

    pub fn unregister_input(&mut self, input_id: &InputId) -> Result<(), UnregisterInputError> {
        if !self.inputs.contains_key(input_id) {
            return Err(UnregisterInputError::NotFound(input_id.clone()));
        }

        self.inputs.remove(input_id);
        self.queue.remove_input(input_id);
        self.renderer.unregister_input(input_id);
        for output in self.outputs.values_mut() {
            if let Some(ref mut cond) = output.audio_end_condition {
                cond.on_input_unregistered(input_id);
            }
            if let Some(ref mut cond) = output.video_end_condition {
                cond.on_input_unregistered(input_id);
            }
        }
        Ok(())
    }

    pub fn register_output(
        pipeline: &Arc<Mutex<Self>>,
        output_id: OutputId,
        register_options: RegisterOutputOptions,
    ) -> Result<Option<Port>, RegisterOutputError> {
        register_pipeline_output(
            pipeline,
            output_id,
            register_options.video,
            register_options.audio,
            |ctx, output_id| new_external_output(ctx, output_id, register_options.output_options),
        )
    }

    pub fn register_encoded_data_output(
        pipeline: &Arc<Mutex<Self>>,
        output_id: OutputId,
        register_options: RegisterEncodedDataOutputOptions,
    ) -> Result<Receiver<EncodedOutputEvent>, RegisterOutputError> {
        register_pipeline_output(
            pipeline,
            output_id,
            register_options.video,
            register_options.audio,
            |ctx, output_id| {
                let (output, result) =
                    EncodedDataOutput::new(output_id, ctx, register_options.output_options)?;
                Ok((Box::new(output), result))
            },
        )
    }

    pub fn register_raw_data_output(
        pipeline: &Arc<Mutex<Self>>,
        output_id: OutputId,
        register_options: RegisterRawDataOutputOptions,
    ) -> Result<RawDataOutputReceiver, RegisterOutputError> {
        register_pipeline_output(
            pipeline,
            output_id,
            register_options.video,
            register_options.audio,
            |_ctx, _output_id| {
                let (output, result) = RawDataOutput::new(register_options.output_options)?;
                Ok((Box::new(output), result))
            },
        )
    }

    pub fn unregister_output(&mut self, output_id: &OutputId) -> Result<(), UnregisterOutputError> {
        if !self.outputs.contains_key(output_id) {
            return Err(UnregisterOutputError::NotFound(output_id.clone()));
        }

        self.audio_mixer.unregister_output(output_id);
        self.outputs.remove(output_id);
        self.renderer.unregister_output(output_id);
        Ok(())
    }

    pub fn register_renderer(
        pipeline: &Arc<Mutex<Self>>,
        renderer_id: RendererId,
        transformation_spec: RendererSpec,
    ) -> Result<(), RegisterRendererError> {
        let renderer = pipeline.lock().unwrap().renderer.clone();
        renderer.register_renderer(renderer_id, transformation_spec)?;
        Ok(())
    }

    pub fn unregister_renderer(
        &self,
        renderer_id: &RendererId,
        registry_type: RegistryType,
    ) -> Result<(), UnregisterRendererError> {
        self.renderer
            .unregister_renderer(renderer_id, registry_type)
    }

    pub fn update_output(
        &mut self,
        output_id: OutputId,
        video: Option<Component>,
        audio: Option<AudioMixerConfig>,
    ) -> Result<(), UpdateSceneError> {
        self.check_output_spec(&output_id, &video, &audio)?;
        if let Some(video) = video {
            self.update_scene_root(output_id.clone(), video)?;
        }

        if let Some(audio) = audio {
            self.update_audio(&output_id, audio)?;
        }

        Ok(())
    }

    pub fn request_keyframe(&self, output_id: OutputId) -> Result<(), RequestKeyframeError> {
        let Some(output) = self.outputs.get(&output_id) else {
            return Err(RequestKeyframeError::OutputNotRegistered(output_id.clone()));
        };

        match output.output.video() {
            Some(video) => video
                .keyframe_request_sender
                .send(())
                .map_err(|_| RequestKeyframeError::KeyframesUnsupported(output_id.clone())),
            None => Err(RequestKeyframeError::NoVideoOutput(output_id.clone())),
        }
    }

    pub fn register_font(&self, font_source: fontdb::Source) {
        self.renderer.register_font(font_source);
    }

    fn check_output_spec(
        &self,
        output_id: &OutputId,
        video: &Option<Component>,
        audio: &Option<AudioMixerConfig>,
    ) -> Result<(), UpdateSceneError> {
        let Some(output) = self.outputs.get(output_id) else {
            return Err(UpdateSceneError::OutputNotRegistered(output_id.clone()));
        };
        if output.audio_end_condition.is_some() != audio.is_some()
            || output.video_end_condition.is_some() != video.is_some()
        {
            return Err(UpdateSceneError::AudioVideoNotMatching(output_id.clone()));
        }
        if video.is_none() && audio.is_none() {
            return Err(UpdateSceneError::NoAudioAndVideo(output_id.clone()));
        }
        Ok(())
    }

    fn update_scene_root(
        &mut self,
        output_id: OutputId,
        scene_root: Component,
    ) -> Result<(), UpdateSceneError> {
        let output = self
            .outputs
            .get(&output_id)
            .ok_or_else(|| UpdateSceneError::OutputNotRegistered(output_id.clone()))?;

        if let Some(cond) = &output.video_end_condition {
            if cond.did_output_end() {
                // Ignore updates after EOS
                warn!("Received output update on a finished output");
                return Ok(());
            }
        }

        let Some(video_output) = output.output.video() else {
            return Err(UpdateSceneError::AudioVideoNotMatching(output_id));
        };

        info!(?output_id, "Update scene {:?}", scene_root);

        self.renderer.update_scene(
            output_id,
            video_output.resolution,
            video_output.frame_format,
            scene_root,
        )
    }

    fn update_audio(
        &mut self,
        output_id: &OutputId,
        audio: AudioMixerConfig,
    ) -> Result<(), UpdateSceneError> {
        let output = self
            .outputs
            .get(output_id)
            .ok_or_else(|| UpdateSceneError::OutputNotRegistered(output_id.clone()))?;

        if let Some(cond) = &output.audio_end_condition {
            if cond.did_output_end() {
                // Ignore updates after EOS
                warn!("Received output update on a finished output");
                return Ok(());
            }
        }

        info!(?output_id, "Update audio mixer {:?}", audio);
        self.audio_mixer.update_output(output_id, audio)
    }

    pub fn start(pipeline: &Arc<Mutex<Self>>) {
        let guard = pipeline.lock().unwrap();
        if guard.is_started {
            error!("Pipeline already started.");
            return;
        }
        info!("Starting pipeline.");
        let (video_sender, video_receiver) = bounded(1);
        let (audio_sender, audio_receiver) = bounded(100);
        guard.queue.start(video_sender, audio_sender);

        let weak_pipeline = Arc::downgrade(pipeline);
        thread::spawn(move || run_renderer_thread(weak_pipeline, video_receiver));

        let weak_pipeline = Arc::downgrade(pipeline);
        thread::spawn(move || run_audio_mixer_thread(weak_pipeline, audio_receiver));
    }

    pub fn inputs(&self) -> impl Iterator<Item = (&InputId, InputInfo)> {
        self.inputs.iter().map(|(id, input)| {
            let protocol = input.input.kind();
            (id, InputInfo { protocol })
        })
    }

    pub fn schedule_event<F: FnOnce(&mut Self) + Send + 'static>(
        pipeline: &Arc<Mutex<Self>>,
        pts: Duration,
        callback: F,
    ) {
        let weak = Arc::downgrade(pipeline);
        let guard = pipeline.lock().unwrap();
        guard.queue.schedule_event(
            pts,
            Box::new(move || {
                let Some(pipeline) = weak.upgrade() else {
                    warn!("Unable to call scheduled callback. Pipeline already dropped.");
                    return;
                };
                let mut guard = pipeline.lock().unwrap();
                callback(&mut guard)
            }),
        );
    }

    pub fn outputs(&self) -> impl Iterator<Item = (&OutputId, OutputInfo)> {
        self.outputs.iter().map(|(id, output)| {
            let protocol = output.output.kind();
            (id, OutputInfo { protocol })
        })
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        info!("Stopping pipeline");
        self.queue.shutdown()
    }
}

fn run_renderer_thread(
    pipeline: Weak<Mutex<Pipeline>>,
    frames_receiver: Receiver<QueueVideoOutput>,
) {
    let renderer = match pipeline.upgrade() {
        Some(pipeline) => pipeline.lock().unwrap().renderer.clone(),
        None => {
            warn!("Pipeline stopped before render thread was started.");
            return;
        }
    };

    for mut input_frames in frames_receiver.iter() {
        let Some(pipeline) = pipeline.upgrade() else {
            break;
        };
        for (input_id, event) in input_frames.frames.iter_mut() {
            if let PipelineEvent::EOS = event {
                let mut guard = pipeline.lock().unwrap();
                if let Some(input) = guard.inputs.get_mut(input_id) {
                    info!(?input_id, "Received video EOS on input.");
                    input.on_video_eos();
                }
                for output in guard.outputs.values_mut() {
                    if let Some(ref mut cond) = output.video_end_condition {
                        cond.on_input_eos(input_id);
                    }
                }
            }
        }

        let output_frame_senders: HashMap<_, _> =
            Pipeline::all_output_video_senders_iter(&pipeline)
                .filter_map(|(output_id, sender)| match sender {
                    OutputSender::ActiveSender(sender) => Some((output_id, sender)),
                    OutputSender::FinishedSender => {
                        renderer.unregister_output(&output_id);
                        None
                    }
                })
                .collect();

        let input_frames: FrameSet<InputId> = input_frames.into();
        trace!(?input_frames, "Rendering frames");
        let output_frames = renderer.render(input_frames);
        let Ok(output_frames) = output_frames else {
            error!(
                "Error while rendering: {}",
                ErrorStack::new(&output_frames.unwrap_err()).into_string()
            );
            continue;
        };

        for (output_id, frame) in output_frames.frames {
            let Some(frame_sender) = output_frame_senders.get(&output_id) else {
                warn!(?output_id, "Received new frame from renderer after EOS.");
                continue;
            };

            if frame_sender.send(PipelineEvent::Data(frame)).is_err() {
                warn!(?output_id, "Failed to send output frames. Channel closed.");
            }
        }
    }
    info!("Stopping renderer thread.")
}

fn run_audio_mixer_thread(
    pipeline: Weak<Mutex<Pipeline>>,
    audio_receiver: Receiver<QueueAudioOutput>,
) {
    let audio_mixer = match pipeline.upgrade() {
        Some(pipeline) => pipeline.lock().unwrap().audio_mixer.clone(),
        None => {
            warn!("Pipeline stopped before mixer thread was started.");
            return;
        }
    };

    for mut samples in audio_receiver.iter() {
        let Some(pipeline) = pipeline.upgrade() else {
            break;
        };
        for (input_id, event) in samples.samples.iter_mut() {
            if let PipelineEvent::EOS = event {
                let mut guard = pipeline.lock().unwrap();
                if let Some(input) = guard.inputs.get_mut(input_id) {
                    info!(?input_id, "Received audio EOS on input.");
                    input.on_audio_eos();
                }
                for output in guard.outputs.values_mut() {
                    if let Some(ref mut cond) = output.audio_end_condition {
                        cond.on_input_eos(input_id);
                    }
                }
            }
        }

        let output_samples_senders: HashMap<_, _> =
            Pipeline::all_output_audio_senders_iter(&pipeline)
                .filter_map(|(output_id, sender)| match sender {
                    OutputSender::ActiveSender(sender) => Some((output_id, sender)),
                    OutputSender::FinishedSender => {
                        audio_mixer.unregister_output(&output_id);
                        None
                    }
                })
                .collect();

        let mixed_samples = audio_mixer.mix_samples(samples.into());

        for (output_id, batch) in mixed_samples.0 {
            let Some(samples_sender) = output_samples_senders.get(&output_id) else {
                warn!(?output_id, "Received new mixed samples after EOS.");
                continue;
            };

            if samples_sender.send(PipelineEvent::Data(batch)).is_err() {
                warn!(?output_id, "Failed to send mixed audio. Channel closed.");
            }
        }
    }
    info!("Stopping audio mixer thread.")
}

fn create_pipeline(opts: PipelineOptions) -> Result<Pipeline, InitPipelineError> {
    let queue_options = QueueOptions::from(&opts);

    let graphics_context = match opts.wgpu_options {
        PipelineWgpuOptions::Context(ctx) => ctx,
        PipelineWgpuOptions::Options {
            features,
            force_gpu,
        } => GraphicsContext::new(GraphicsContextOptions {
            force_gpu,
            features,
            ..Default::default()
        })?,
    };

    let renderer = Renderer::new(RendererOptions {
        chromium_context: opts.chromium_context,
        framerate: opts.output_framerate,
        stream_fallback_timeout: opts.stream_fallback_timeout,
        load_system_fonts: opts.load_system_fonts,
        device: graphics_context.device.clone(),
        queue: graphics_context.queue.clone(),
        rendering_mode: opts.rendering_mode,
    })?;

    let download_dir = opts
        .download_root
        .join(format!("smelter-{}", rand::random::<u64>()))
        .into();
    std::fs::create_dir_all(&download_dir).map_err(InitPipelineError::CreateDownloadDir)?;

    let tokio_rt = match opts.tokio_rt {
        Some(tokio_rt) => tokio_rt,
        None => Arc::new(Runtime::new().map_err(InitPipelineError::CreateTokioRuntime)?),
    };

    let ctx = Arc::new(PipelineCtx {
        queue_sync_point: Instant::now(),
        default_buffer_duration: opts.default_buffer_duration,

        mixing_sample_rate: opts.mixing_sample_rate,
        output_framerate: opts.output_framerate,

        stun_servers: opts.whip_whep_stun_servers.clone(),
        download_dir,
        event_emitter: Arc::new(EventEmitter::new()),
        tokio_rt: tokio_rt.clone(),
        graphics_context,
        whip_whep_state: match opts.whip_whep_server {
            PipelineWhipWhepServerOptions::Enable { port } => {
                Some(WhipWhepPipelineState::new(port))
            }
            PipelineWhipWhepServerOptions::Disable => None,
        },
    });

    let whip_whep_handle = match &ctx.whip_whep_state {
        Some(state) => Some(WhipWhepServer::spawn(ctx.clone(), state)?),
        None => None,
    };

    let pipeline = Pipeline {
        outputs: HashMap::new(),
        inputs: HashMap::new(),
        queue: Queue::new(queue_options, &ctx),
        renderer,
        audio_mixer: AudioMixer::new(opts.mixing_sample_rate),
        is_started: false,
        ctx,
        whip_whep_handle,
    };

    Ok(pipeline)
}
