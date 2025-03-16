// This example illustrates how to initialize a GraphicsContext separately to get access to a wgpu
// instance, adapter, queue and device.

#[cfg(target_os = "linux")]
fn main() {
    use compositor_pipeline::{
        pipeline::{GraphicsContext, Options},
        Pipeline,
    };
    use compositor_render::RenderingMode;
    use smelter::config::read_config;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    let graphics_context = GraphicsContext::new(Default::default()).unwrap();

    let _device = graphics_context.device.clone();
    let _queue = graphics_context.queue.clone();
    let _adapter = graphics_context.adapter.clone();
    let _instance = graphics_context.instance.clone();

    let config = read_config();

    let _pipeline = Pipeline::new(Options {
        wgpu_ctx: Some(graphics_context),
        queue_options: config.queue_options,
        stream_fallback_timeout: config.stream_fallback_timeout,
        web_renderer: config.web_renderer,
        force_gpu: config.force_gpu,
        download_root: config.download_root,
        mixing_sample_rate: config.mixing_sample_rate,
        stun_servers: config.stun_servers,
        wgpu_features: config.required_wgpu_features,
        load_system_fonts: Some(true),
        tokio_rt: Some(Arc::new(Runtime::new().unwrap())),
        whip_whep_server_port: Some(config.whip_whep_server_port),
        start_whip_whep: config.start_whip_whep,
        rendering_mode: RenderingMode::GpuOptimized,
    })
    .unwrap();
}

#[cfg(target_os = "macos")]
fn main() {}
