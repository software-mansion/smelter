// This example illustrates how to initialize a GraphicsContext separately to get access to a wgpu
// instance, adapter, queue and device.

#[cfg(target_os = "linux")]
fn main() {
    use compositor_pipeline::{graphics_context::GraphicsContext, Pipeline, PipelineOptions};
    use smelter::config::read_config;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    let graphics_context = GraphicsContext::new(Default::default()).unwrap();

    let _device = graphics_context.device.clone();
    let _queue = graphics_context.queue.clone();
    let _adapter = graphics_context.adapter.clone();
    let _instance = graphics_context.instance.clone();

    let config = read_config();

    let _pipeline = Pipeline::new(PipelineOptions {
        wgpu_ctx: Some(graphics_context),
        tokio_rt: Some(Arc::new(Runtime::new().unwrap())),
        ..(&config).into()
    })
    .unwrap();
}

#[cfg(target_os = "macos")]
fn main() {}
