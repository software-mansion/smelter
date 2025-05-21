use std::sync::Arc;

use wasm_bindgen::JsValue;

use super::types::to_js_error;

pub async fn create_wgpu_context() -> Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>), JsValue> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL,
        backend_options: wgpu::BackendOptions {
            gl: wgpu::GlBackendOptions {
                // Default behavior for fences on WebGL breaks `device.poll(Wait)` (always timeouts).
                // `AutoFinish` makes the web behavior consistent with native at the cost of `Queue::on_completed_work_done` not working correctly.
                // https://github.com/gfx-rs/wgpu/blob/a95c69eb910c78306c4f19212183177f51f99aea/wgpu-types/src/instance.rs#L548-L560
                fence_behavior: wgpu::GlFenceBehavior::AutoFinish,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });

    let canvas = web_sys::OffscreenCanvas::new(0, 0)?;
    let surface_target = wgpu::SurfaceTarget::OffscreenCanvas(canvas);
    let surface = instance
        .create_surface(surface_target)
        .map_err(to_js_error)?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .map_err(|_| JsValue::from_str("Failed to get a wgpu adapter"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::PUSH_CONSTANTS,
            required_limits: wgpu::Limits {
                max_push_constant_size: 128,
                max_color_attachments: 6,
                ..wgpu::Limits::downlevel_webgl2_defaults()
            },
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(to_js_error)?;

    Ok((device.into(), queue.into()))
}

pub fn pad_to_256(value: u32) -> u32 {
    if value % 256 == 0 {
        value
    } else {
        value + (256 - (value % 256))
    }
}
