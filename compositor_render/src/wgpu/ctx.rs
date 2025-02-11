use std::sync::Arc;

use log::{error, info};

use super::{
    common_pipeline::plane::Plane, format::TextureFormat, texture::Texture, utils::TextureUtils,
    CreateWgpuCtxError, WgpuErrorScope,
};

#[derive(Debug)]
pub struct WgpuCtx {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,

    pub shader_header: naga::Module,

    pub format: TextureFormat,
    pub utils: TextureUtils,

    pub uniform_bgl: wgpu::BindGroupLayout,
    pub plane: Plane,
    pub empty_texture: Texture,
}

impl WgpuCtx {
    pub fn new(
        force_gpu: bool,
        features: wgpu::Features,
        override_wgpu_ctx: Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>,
    ) -> Result<Arc<Self>, CreateWgpuCtxError> {
        let ctx = match override_wgpu_ctx {
            Some((device, queue)) => {
                Self::check_wgpu_ctx(&device, features);
                Self::new_from_device_queue(device, queue)?
            }
            None => {
                let WgpuComponents { device, queue, .. } =
                    create_wgpu_ctx(force_gpu, features, Default::default(), None)?;
                Self::new_from_device_queue(device, queue)?
            }
        };
        Ok(Arc::new(ctx))
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
    ) -> Result<Self, CreateWgpuCtxError> {
        let shader_header = crate::transformations::shader::validation::shader_header();

        let scope = WgpuErrorScope::push(&device);

        let format = TextureFormat::new(&device);
        let utils = TextureUtils::new(&device);

        let uniform_bgl = uniform_bind_group_layout(&device);

        let plane = Plane::new(&device);
        let empty_texture = Texture::empty(&device);

        scope.pop(&device)?;

        device.on_uncaptured_error(Box::new(|e| {
            error!("wgpu error: {:?}", e);
        }));

        Ok(Self {
            device,
            queue,
            shader_header,
            format,
            utils,
            uniform_bgl,
            plane,
            empty_texture,
        })
    }
}

pub fn required_wgpu_features() -> wgpu::Features {
    match cfg!(target_arch = "wasm32") {
        false => wgpu::Features::TEXTURE_BINDING_ARRAY | wgpu::Features::PUSH_CONSTANTS,
        true => wgpu::Features::PUSH_CONSTANTS,
    }
}

pub fn set_required_wgpu_limits(limits: wgpu::Limits) -> wgpu::Limits {
    wgpu::Limits {
        max_push_constant_size: limits.max_push_constant_size.max(128),
        ..limits
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
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    #[cfg(not(target_arch = "wasm32"))]
    log_available_adapters(&instance);

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptionsBase {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface,
    }))
    .ok_or(CreateWgpuCtxError::NoAdapter)?;

    let adapter_info = adapter.get_info();
    info!(
        "Using {} adapter with {:?} backend",
        adapter_info.name, adapter_info.backend
    );
    if force_gpu && adapter_info.device_type != wgpu::DeviceType::Cpu {
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

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_limits: set_required_wgpu_limits(limits),
            required_features,
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))?;
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
