use std::{collections::HashMap, sync::Arc, time::Instant};

use compositor_render::{EventLoop, Renderer, RendererOptions};
use tokio::runtime::Runtime;

use crate::{
    audio_mixer::AudioMixer,
    error::InitPipelineError,
    event::EventEmitter,
    pipeline::webrtc::{WhipWhepPipelineState, WhipWhepServer},
    queue::Queue,
};

use super::{GraphicsContext, GraphicsContextOptions, Options, Pipeline, PipelineCtx};

pub(super) fn create_pipeline(
    opts: Options,
) -> Result<(Pipeline, Arc<dyn EventLoop>), InitPipelineError> {
    let graphics_context = match opts.wgpu_ctx {
        Some(ctx) => ctx,
        None => GraphicsContext::new(GraphicsContextOptions {
            force_gpu: opts.force_gpu,
            features: opts.wgpu_features,
            ..Default::default()
        })?,
    };

    let (renderer, event_loop) = Renderer::new(RendererOptions {
        web_renderer: opts.web_renderer,
        framerate: opts.queue_options.output_framerate,
        stream_fallback_timeout: opts.stream_fallback_timeout,
        load_system_fonts: opts.load_system_fonts.unwrap_or(true),
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
        queue_sync_time: Instant::now(),
        mixing_sample_rate: opts.mixing_sample_rate,
        output_framerate: opts.queue_options.output_framerate,
        stun_servers: opts.stun_servers.clone(),
        download_dir,
        event_emitter: Arc::new(EventEmitter::new()),
        tokio_rt: tokio_rt.clone(),
        graphics_context,
        whip_whep_state: match opts.start_whip_whep {
            true => Some(WhipWhepPipelineState::default().into()),
            false => None,
        },
    });

    let whip_whep_handle = match &ctx.whip_whep_state {
        Some(state) => Some(WhipWhepServer::spawn(
            ctx.clone(),
            state,
            opts.whip_whep_server_port,
        )?),
        None => None,
    };

    let pipeline = Pipeline {
        outputs: HashMap::new(),
        inputs: HashMap::new(),
        queue: Queue::new(opts.queue_options, &ctx.event_emitter),
        renderer,
        audio_mixer: AudioMixer::new(opts.mixing_sample_rate),
        is_started: false,
        ctx,
        whip_whep_handle,
    };

    Ok((pipeline, event_loop))
}
