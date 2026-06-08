#![doc = include_str!("../README.md")]

#[cfg(vulkan)]
mod adapter;
#[cfg(vulkan)]
pub(crate) mod codec;
#[cfg(vulkan)]
mod device;
#[cfg(vulkan)]
mod instance;
#[cfg(vulkan)]
mod vulkan_decoder;
#[cfg(vulkan)]
mod vulkan_encoder;
#[cfg(all(vulkan, feature = "transcoder"))]
mod vulkan_transcoder;
#[cfg(all(vulkan, feature = "wgpu"))]
pub(crate) mod wgpu_helpers;
#[cfg(vulkan)]
pub(crate) mod wrappers;

#[cfg(vulkan)]
mod vulkan_video;
#[cfg(vulkan)]
pub use vulkan_video::*;

mod types;
pub use types::{VideoFramerate, VideoResolution};

#[cfg(feature = "expose-parsers")]
pub mod parser;
#[cfg(not(feature = "expose-parsers"))]
pub(crate) mod parser;

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
mod dmabuf;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
pub use dmabuf::{
    DRM_FORMAT_NV12, DmaBufFrame, DmaBufLayer, DmaBufObject, DmaBufPlane,
    Nv12DmaBufImportUsage, export_nv12_dmabuf_texture, import_nv12_dmabuf_texture,
    validate_nv12_dmabuf_frame, validate_nv12_dmabuf_layout,
};

#[cfg(all(feature = "vaapi", target_os = "linux"))]
pub mod vaapi;

#[cfg(all(test, target_os = "linux", feature = "wgpu"))]
type TestWgpuDeviceAndQueue = (std::sync::Arc<wgpu::Device>, wgpu::Queue, wgpu::AdapterInfo);

#[cfg(all(test, target_os = "linux", feature = "wgpu"))]
fn test_wgpu_device_and_queue() -> TestWgpuDeviceAndQueue {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .expect("failed to find WGPU adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::TEXTURE_FORMAT_NV12,
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: unsafe { wgpu::ExperimentalFeatures::enabled() },
            trace: wgpu::Trace::Off,
        }))
        .expect("failed to create WGPU device");
    (std::sync::Arc::new(device), queue, adapter.get_info())
}

#[cfg(all(feature = "dmabuf", not(target_os = "linux")))]
compile_error!("gpu-video DMA-BUF support can be only compiled on Linux.");

#[cfg(all(feature = "vaapi", not(target_os = "linux")))]
compile_error!("gpu-video VA-API support can be only compiled on Linux.");
