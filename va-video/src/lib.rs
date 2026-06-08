#![doc = "VA-API video coding helpers with wgpu DMA-BUF integration."]

#[cfg(target_os = "linux")]
mod display;
pub mod h264;

#[cfg(all(test, target_os = "linux"))]
fn test_wgpu_device_and_queue()
-> (std::sync::Arc<wgpu::Device>, wgpu::Queue, wgpu::AdapterInfo) {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .expect("failed to find WGPU adapter");
    let required_features =
        smelter_render::required_wgpu_features() | wgpu::Features::TEXTURE_FORMAT_NV12;
    let required_limits =
        smelter_render::set_required_wgpu_limits(wgpu::Limits::default());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features,
            required_limits,
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: unsafe { wgpu::ExperimentalFeatures::enabled() },
            trace: wgpu::Trace::Off,
        }))
        .expect("failed to create WGPU device");
    (std::sync::Arc::new(device), queue, adapter.get_info())
}
