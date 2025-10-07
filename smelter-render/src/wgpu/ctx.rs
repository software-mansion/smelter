use std::sync::Arc;

use log::error;

use crate::RenderingMode;

use super::{
    CreateWgpuCtxError, WgpuErrorScope,
    common_pipeline::plane::Plane,
    format::TextureFormat,
    texture::{RgbaLinearTexture, RgbaSrgbTexture},
    utils::TextureUtils,
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
    match cfg!(target_arch = "wasm32") {
        false => wgpu::Features::TEXTURE_BINDING_ARRAY | wgpu::Features::PUSH_CONSTANTS,
        true => wgpu::Features::PUSH_CONSTANTS,
    }
}

pub fn set_required_wgpu_limits(limits: wgpu::Limits) -> wgpu::Limits {
    wgpu::Limits {
        max_binding_array_elements_per_shader_stage: limits
            .max_binding_array_elements_per_shader_stage
            .max(128),
        max_push_constant_size: limits.max_push_constant_size.max(128),
        ..limits
    }
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
