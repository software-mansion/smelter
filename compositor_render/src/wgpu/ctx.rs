use std::sync::Arc;

use log::{error, info};

use crate::RenderingMode;

use super::{
    common_pipeline::plane::Plane,
    format::TextureFormat,
    texture::{RgbaLinearTexture, RgbaSrgbTexture},
    utils::TextureUtils,
    CreateWgpuCtxError, WgpuErrorScope,
};

#[derive(Debug)]
pub struct WgpuCtx {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,

    pub mode: RenderingMode,

    pub shader_header: wgpu::naga::Module,

    pub format: TextureFormat,
    pub utils: TextureUtils,

    pub uniform_bgl: wgpu::BindGroupLayout,
    pub plane: Plane,
    pub empty_rgba_linear_texture: RgbaLinearTexture,
    pub empty_rgba_srgb_texture: RgbaSrgbTexture,
}

impl WgpuCtx {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        mode: RenderingMode,
    ) -> Result<Arc<Self>, CreateWgpuCtxError> {
        Self::check_wgpu_ctx(&device, required_wgpu_features());
        let ctx = Self::new_from_device_queue(device, queue, mode)?;
        Ok(Arc::new(ctx))
    }

    pub fn default_empty_view(&self) -> &wgpu::TextureView {
        match self.mode {
            RenderingMode::GpuOptimized => self.empty_rgba_srgb_texture.view(),
            RenderingMode::CpuOptimized => self.empty_rgba_linear_texture.view(),
            RenderingMode::WebGl => self.empty_rgba_srgb_texture.view(),
        }
    }

    pub fn default_view_format(&self) -> wgpu::TextureFormat {
        match self.mode {
            RenderingMode::GpuOptimized => wgpu::TextureFormat::Rgba8UnormSrgb,
            RenderingMode::CpuOptimized => wgpu::TextureFormat::Rgba8Unorm,
            RenderingMode::WebGl => wgpu::TextureFormat::Rgba8UnormSrgb,
        }
    }

    fn check_wgpu_ctx(device: &wgpu::Device, features: wgpu::Features) {
        let expected_features = features | required_wgpu_features();

        let missing_features = expected_features.difference(device.features());
        if !missing_features.is_empty() {
            error!(
                "Provided wgpu::Device does not support following features: {missing_features:?}"
            );
        }
    }

    fn new_from_device_queue(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        mode: RenderingMode,
    ) -> Result<Self, CreateWgpuCtxError> {
        let shader_header = crate::transformations::shader::validation::shader_header();

        let scope = WgpuErrorScope::push(&device);

        let format = TextureFormat::new(&device);
        let utils = TextureUtils::new(&device, &format);

        let uniform_bgl = uniform_bind_group_layout(&device);

        let plane = Plane::new(&device);
        let empty_rgba_linear_texture = RgbaLinearTexture::empty(&device);
        let empty_rgba_srgb_texture = RgbaSrgbTexture::empty(&device);

        scope.pop(&device)?;

        device.on_uncaptured_error(Box::new(|e| {
            error!("wgpu error: {:?}", e);
        }));

        Ok(Self {
            mode,
            device,
            queue,
            shader_header,
            format,
            utils,
            uniform_bgl,
            plane,
            empty_rgba_linear_texture,
            empty_rgba_srgb_texture,
        })
    }
}

pub fn required_wgpu_features() -> wgpu::Features {
    wgpu::Features::PUSH_CONSTANTS
}

pub fn set_required_wgpu_limits(limits: wgpu::Limits) -> wgpu::Limits {
    wgpu::Limits {
        max_binding_array_elements_per_shader_stage: limits
            .max_binding_array_elements_per_shader_stage
            .max(128),
        max_push_constant_size: limits.max_push_constant_size.max(128),
        ..wgpu::Limits::downlevel_defaults()
    }
}

#[derive(Clone)]
pub struct WgpuComponents {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub adapter: Arc<wgpu::Adapter>,
    pub instance: Arc<wgpu::Instance>,
}

pub fn create_wgpu_ctx(
    force_gpu: bool,
    features: wgpu::Features,
    limits: wgpu::Limits,
    compatible_surface: Option<&wgpu::Surface<'_>>,
) -> Result<WgpuComponents, CreateWgpuCtxError> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });

    #[cfg(not(target_arch = "wasm32"))]
    log_available_adapters(&instance);

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptionsBase {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface,
    }))?;

    let adapter_info = adapter.get_info();
    info!(
        "Using {} adapter with {:?} backend",
        adapter_info.name, adapter_info.backend
    );
    if force_gpu && adapter_info.device_type == wgpu::DeviceType::Cpu {
        error!("Selected adapter is CPU based. Aborting.");
        return Err(CreateWgpuCtxError::NoAdapter);
    }
    let required_features = features | required_wgpu_features();

    let missing_features = required_features.difference(adapter.features());
    if !missing_features.is_empty() {
        error!("Selected adapter or its driver does not support required wgpu features. Missing features: {missing_features:?}).");
        error!("You can configure some of the required features using \"SMELTER_REQUIRED_WGPU_FEATURES\" environment variable. Check https://smelter.dev/docs for more.");
        return Err(CreateWgpuCtxError::NoAdapter);
    }

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: None,
        required_limits: set_required_wgpu_limits(limits),
        required_features,
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))?;
    Ok(WgpuComponents {
        instance: instance.into(),
        adapter: adapter.into(),
        device: device.into(),
        queue: queue.into(),
    })
}

fn uniform_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("uniform bind group layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            count: None,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        }],
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn log_available_adapters(instance: &wgpu::Instance) {
    let adapters: Vec<_> = instance
        .enumerate_adapters(wgpu::Backends::all())
        .iter()
        .map(|adapter| {
            let info = adapter.get_info();
            format!("\n - {info:?}")
        })
        .collect();
    info!("Available adapters: {}", adapters.join(""))
}
