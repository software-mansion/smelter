// This example illustrates how to initialize a GraphicsContext separately to get access to a wgpu
// instance, adapter, queue and device.

#[cfg(target_os = "linux")]
fn main() {
    use smelter::{config::read_config, state::pipeline_options_from_config};
    use smelter_core::{graphics_context::GraphicsContext, Pipeline, PipelineWgpuOptions};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    let graphics_context = GraphicsContext::new(Default::default()).unwrap();

    let _device = graphics_context.device.clone();
    let _queue = graphics_context.queue.clone();
    let _adapter = graphics_context.adapter.clone();
    let _instance = graphics_context.instance.clone();

    let config = read_config();

    let mut options =
        pipeline_options_from_config(&config, &Arc::new(Runtime::new().unwrap()), &None);
    options.wgpu_options = PipelineWgpuOptions::Context(graphics_context);
    let _pipeline = Pipeline::new(options).unwrap();
}

#[cfg(target_os = "macos")]
fn main() {}
