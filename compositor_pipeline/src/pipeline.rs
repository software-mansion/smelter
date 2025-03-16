use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;
use std::thread;
use std::time::Duration;

use compositor_render::error::{
    ErrorStack, RegisterRendererError, RequestKeyframeError, UnregisterRendererError,
};
use compositor_render::scene::Component;
use compositor_render::web_renderer::WebRendererInitOptions;
use compositor_render::FrameSet;
use compositor_render::Framerate;
use compositor_render::RegistryType;
use compositor_render::RendererOptions;
use compositor_render::RenderingMode;
use compositor_render::WgpuFeatures;
use compositor_render::{error::UpdateSceneError, Renderer};
use compositor_render::{EventLoop, InputId, OutputId, RendererId, RendererSpec};
use crossbeam_channel::{bounded, Receiver};
use glyphon::fontdb;
use input::InputInitInfo;
use input::RawDataInputOptions;
use output::EncodedDataOutputOptions;
use output::OutputOptions;
use output::RawDataOutputOptions;
use pipeline_output::register_pipeline_output;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tracing::{error, info, trace, warn};
use types::RawDataSender;
use whip_whep::run_whip_whep_server;
use whip_whep::WhipWhepState;

use crate::audio_mixer::AudioMixer;
use crate::audio_mixer::MixingStrategy;
use crate::audio_mixer::{AudioChannels, AudioMixingParams};
use crate::error::InitPipelineError;
use crate::error::{
    RegisterInputError, RegisterOutputError, UnregisterInputError, UnregisterOutputError,
};

use crate::event::Event;
use crate::event::EventEmitter;
use crate::pipeline::pipeline_output::OutputSender;
use crate::queue::PipelineEvent;
use crate::queue::QueueAudioOutput;
use crate::queue::QueueInputOptions;
use crate::queue::{self, Queue, QueueOptions, QueueVideoOutput};

use self::input::InputOptions;

pub mod decoder;
pub mod encoder;
mod graphics_context;
pub mod input;
pub mod output;
mod pipeline_input;
mod pipeline_output;
pub mod rtp;
mod types;
pub mod whip_whep;

use self::pipeline_input::register_pipeline_input;
use self::pipeline_input::PipelineInput;
use self::pipeline_output::PipelineOutput;

pub use self::types::{
    AudioCodec, EncodedChunk, EncodedChunkKind, EncoderOutputEvent, RawDataReceiver, VideoCodec,
    VideoDecoder,
};
pub use pipeline_output::PipelineOutputEndCondition;

pub use graphics_context::{GraphicsContext, GraphicsContextOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(pub u16);

pub struct RegisterInputOptions {
    pub input_options: InputOptions,
    pub queue_options: queue::QueueInputOptions,
}

#[derive(Debug, Clone)]
pub struct RegisterOutputOptions<T> {
    pub output_options: T,
    pub video: Option<OutputVideoOptions>,
    pub audio: Option<OutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct OutputVideoOptions {
    pub initial: Component,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct OutputAudioOptions {
    pub initial: AudioMixingParams,
    pub mixing_strategy: MixingStrategy,
    pub channels: AudioChannels,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct OutputScene {
    pub output_id: OutputId,
    pub scene_root: Component,
}

pub struct Pipeline {
    ctx: PipelineCtx,
    inputs: HashMap<InputId, PipelineInput>,
    outputs: HashMap<OutputId, PipelineOutput>,
    queue: Arc<Queue>,
    renderer: Renderer,
    audio_mixer: AudioMixer,
    is_started: bool,
    shutdown_whip_whep_sender: Option<oneshot::Sender<()>>,
}

#[derive(Debug)]
pub struct Options {
    pub queue_options: QueueOptions,
    pub stream_fallback_timeout: Duration,
    pub web_renderer: WebRendererInitOptions,
    pub force_gpu: bool,
    pub download_root: PathBuf,
    pub mixing_sample_rate: u32,
    pub stun_servers: Arc<Vec<String>>,
    pub wgpu_features: WgpuFeatures,
    pub load_system_fonts: Option<bool>,
    pub wgpu_ctx: Option<GraphicsContext>,
    pub whip_whep_server_port: Option<u16>,
    pub start_whip_whep: bool,
    pub tokio_rt: Option<Arc<Runtime>>,
    pub rendering_mode: RenderingMode,
}

#[derive(Clone)]
pub struct PipelineCtx {
    pub mixing_sample_rate: u32,
    pub output_framerate: Framerate,
    pub stun_servers: Arc<Vec<String>>,
    pub download_dir: Arc<PathBuf>,
    pub event_emitter: Arc<EventEmitter>,
    pub whip_whep_state: Arc<WhipWhepState>,
    pub tokio_rt: Arc<Runtime>,
    pub start_whip_whep: bool,
    #[cfg(feature = "vk-video")]
    pub vulkan_ctx: Option<graphics_context::VulkanCtx>,
}

impl std::fmt::Debug for PipelineCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineCtx")
            .field("mixing_sample_rate", &self.mixing_sample_rate)
            .field("output_framerate", &self.output_framerate)
            .field("download_dir", &self.download_dir)
            .field("event_emitter", &self.event_emitter)
            .finish()
    }
}

impl Pipeline {
    pub fn new(opts: Options) -> Result<(Self, Arc<dyn EventLoop>), InitPipelineError> {
        let preinitialized_ctx = match opts.wgpu_ctx {
            Some(ctx) => Some(ctx),
            #[cfg(feature = "vk-video")]
            None => Some(GraphicsContext::new(GraphicsContextOptions {
                force_gpu: opts.force_gpu,
                features: opts.wgpu_features,
                ..Default::default()
            })?),
            #[cfg(not(feature = "vk-video"))]
            None => None,
        };

        let wgpu_ctx = preinitialized_ctx
            .as_ref()
            .map(|ctx| (ctx.device.clone(), ctx.queue.clone()));

        let (renderer, event_loop) = Renderer::new(RendererOptions {
            web_renderer: opts.web_renderer,
            framerate: opts.queue_options.output_framerate,
            stream_fallback_timeout: opts.stream_fallback_timeout,
            force_gpu: opts.force_gpu,
            wgpu_features: opts.wgpu_features,
            load_system_fonts: opts.load_system_fonts.unwrap_or(true),
            wgpu_ctx,
            rendering_mode: opts.rendering_mode,
        })?;

        let download_dir = opts
            .download_root
            .join(format!("smelter-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&download_dir).map_err(InitPipelineError::CreateDownloadDir)?;

        let tokio_rt = match opts.tokio_rt {
            Some(tokio_rt) => tokio_rt,
            None => Arc::new(Runtime::new().map_err(InitPipelineError::CreateTokioRuntime)?),
        };
        let stun_servers = opts.stun_servers;
        let whip_whep_state = WhipWhepState::new(stun_servers.clone());
        let start_whip_whep = opts.start_whip_whep;
        let shutdown_whip_whep_sender = if start_whip_whep {
            if let Some(port) = opts.whip_whep_server_port {
                let whip_whep_state = whip_whep_state.clone();
                let (sender, receiver) = oneshot::channel();
                let (init_result_sender, init_result_receiver) = oneshot::channel();
                tokio_rt.spawn(async move {
                    run_whip_whep_server(port, whip_whep_state, receiver, init_result_sender).await
                });
                match init_result_receiver.blocking_recv() {
                    Ok(init_result) => init_result?,
                    Err(err) => {
                        error!(
                            "Error while receiving WHIP WHEP server initialization result {:?}",
                            err
                        )
                    }
                }
                Some(sender)
            } else {
                error!("WHIP WHEP server port not specified.");
                None
            }
        } else {
            None
        };
        let event_emitter = Arc::new(EventEmitter::new());
        let pipeline = Pipeline {
            outputs: HashMap::new(),
            inputs: HashMap::new(),
            queue: Queue::new(opts.queue_options, &event_emitter),
            renderer,
            audio_mixer: AudioMixer::new(opts.mixing_sample_rate),
            is_started: false,
            shutdown_whip_whep_sender,
            ctx: PipelineCtx {
                mixing_sample_rate: opts.mixing_sample_rate,
                output_framerate: opts.queue_options.output_framerate,
                stun_servers,
                download_dir: download_dir.into(),
                event_emitter,
                tokio_rt,
                whip_whep_state,
                start_whip_whep,
                #[cfg(feature = "vk-video")]
                vulkan_ctx: preinitialized_ctx.and_then(|ctx| ctx.vulkan_ctx),
            },
        };

        Ok((pipeline, event_loop))
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn subscribe_pipeline_events(&self) -> Receiver<Event> {
        self.ctx.event_emitter.subscribe()
    }

    pub fn register_input(
        pipeline: &Arc<Mutex<Self>>,
        input_id: InputId,
        register_options: RegisterInputOptions,
    ) -> Result<InputInitInfo, RegisterInputError> {
        register_pipeline_input(
            pipeline,
            input_id,
            &register_options.input_options,
            register_options.queue_options,
        )
    }

    pub fn register_raw_data_input(
        pipeline: &Arc<Mutex<Self>>,
        input_id: InputId,
        raw_input_options: RawDataInputOptions,
        queue_options: QueueInputOptions,
    ) -> Result<RawDataSender, RegisterInputError> {
        register_pipeline_input(pipeline, input_id, &raw_input_options, queue_options)
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
        register_options: RegisterOutputOptions<OutputOptions>,
    ) -> Result<Option<Port>, RegisterOutputError> {
        register_pipeline_output(
            pipeline,
            output_id,
            &register_options.output_options,
            register_options.video,
            register_options.audio,
        )
    }

    pub fn register_encoded_data_output(
        pipeline: &Arc<Mutex<Self>>,
        output_id: OutputId,
        register_options: RegisterOutputOptions<EncodedDataOutputOptions>,
    ) -> Result<Receiver<EncoderOutputEvent>, RegisterOutputError> {
        register_pipeline_output(
            pipeline,
            output_id,
            &register_options.output_options,
            register_options.video,
            register_options.audio,
        )
    }

    pub fn register_raw_data_output(
        pipeline: &Arc<Mutex<Self>>,
        output_id: OutputId,
        register_options: RegisterOutputOptions<RawDataOutputOptions>,
    ) -> Result<RawDataReceiver, RegisterOutputError> {
        register_pipeline_output(
            pipeline,
            output_id,
            &register_options.output_options,
            register_options.video,
            register_options.audio,
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
        audio: Option<AudioMixingParams>,
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

        output.output.request_keyframe(output_id)
    }

    pub fn register_font(&self, font_source: fontdb::Source) {
        self.renderer.register_font(font_source);
    }

    fn check_output_spec(
        &self,
        output_id: &OutputId,
        video: &Option<Component>,
        audio: &Option<AudioMixingParams>,
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

        let (Some(resolution), Some(frame_format)) = (
            output.output.resolution(),
            output.output.output_frame_format(),
        ) else {
            return Err(UpdateSceneError::AudioVideoNotMatching(output_id));
        };

        info!(?output_id, "Update scene {:#?}", scene_root);

        self.renderer
            .update_scene(output_id, resolution, frame_format, scene_root)
    }

    fn update_audio(
        &mut self,
        output_id: &OutputId,
        audio: AudioMixingParams,
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

        info!(?output_id, "Update audio mixer {:#?}", audio);
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

    pub fn inputs(&self) -> impl Iterator<Item = (&InputId, &PipelineInput)> {
        self.inputs.iter()
    }

    pub fn outputs(&self) -> impl Iterator<Item = (&OutputId, &PipelineOutput)> {
        self.outputs.iter()
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        if let Some(sender) = self.shutdown_whip_whep_sender.take() {
            if sender.send(()).is_err() {
                error!("Cannot send shutdown signal to WHIP WHEP server")
            }
        }
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
