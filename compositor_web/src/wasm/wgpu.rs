use std::sync::Arc;

use wasm_bindgen::JsValue;

use super::types::to_js_error;

pub async fn create_wgpu_context() -> Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>), JsValue> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL,
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
        .ok_or(JsValue::from_str("Failed to get a wgpu adapter"))?;
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::PUSH_CONSTANTS,
                required_limits: wgpu::Limits {
                    max_push_constant_size: 128,
                    max_texture_dimension_2d: 4096,
                    ..wgpu::Limits::downlevel_webgl2_defaults()
                },
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
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
