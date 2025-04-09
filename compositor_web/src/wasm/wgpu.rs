use std::sync::Arc;

use compositor_render::Resolution;
use wasm_bindgen::JsValue;
use web_sys::OffscreenCanvas;
use wgpu::util::DeviceExt;

use super::types::to_js_error;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: [f32; 3],
    texture_coords: [f32; 2],
}

impl Vertex {
    const ATTRIBUTES: &[wgpu::VertexAttribute] =
        &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: Self::ATTRIBUTES,
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
    };
}

pub struct Quad {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

impl Quad {
    const VERTICES: &[Vertex] = &[
        Vertex {
            position: [-1.0, 1.0, 0.0],
            texture_coords: [0.0, 0.0],
        },
        Vertex {
            position: [-1.0, -1.0, 0.0],
            texture_coords: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            texture_coords: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            texture_coords: [1.0, 0.0],
        },
    ];

    pub const INDICES: &[u16] = &[0, 1, 3, 1, 2, 3];

    pub fn new(device: &wgpu::Device) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(Self::VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(Self::INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
        }
    }
}

pub struct WgpuContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: wgpu::Surface<'static>,
    pub canvas: OffscreenCanvas,
    pub quad: Quad,
}

impl WgpuContext {
    pub async fn new() -> Result<Self, JsValue> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        let canvas = web_sys::OffscreenCanvas::new(0, 0)?;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
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
                        max_color_attachments: 6,
                        ..wgpu::Limits::downlevel_webgl2_defaults()
                    },
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(to_js_error)?;

        let quad = Quad::new(&device);
        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            canvas,
            quad,
        })
    }

    pub fn ensure_surface_resolution(&self, resolution: Resolution) {
        let width = resolution.width as u32;
        let height = resolution.height as u32;
        if self.canvas.width() == width && self.canvas.height() == height {
            return;
        }

        self.canvas.set_width(width);
        self.canvas.set_height(height);

        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                width,
                height,
                present_mode: wgpu::PresentMode::AutoNoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                view_formats: vec![wgpu::TextureFormat::Rgba8UnormSrgb],
            },
        );
    }
}
