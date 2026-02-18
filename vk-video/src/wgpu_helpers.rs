use std::marker::PhantomData;

mod nv12_to_rgba;
mod rgba_to_nv12;

pub use nv12_to_rgba::*;
pub use rgba_to_nv12::*;

/// Represents mapping between `InputTexture` and `OutputTexture` used by the converters.
/// `InputTexture` is converted into `OutputTexture`.
///
/// Can be created with [`WgpuNv12ToRgbaConverter`] or [`WgpuRgbaToNv12Converter`].
#[derive(Clone)]
pub struct WgpuTextureMapping<InputTexture, OutputTexture> {
    pub(crate) input_bg: wgpu::BindGroup,
    _input_texture: PhantomData<InputTexture>,
    output_texture: OutputTexture,
}

impl<InputTexture, OutputTexture> WgpuTextureMapping<InputTexture, OutputTexture> {
    pub fn output(&self) -> &OutputTexture {
        &self.output_texture
    }
}

#[derive(Clone)]
pub struct Nv12Texture {
    texture: wgpu::Texture,
    y_plane_view: wgpu::TextureView,
    uv_plane_view: wgpu::TextureView,
}

impl Nv12Texture {
    fn new(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::NV12,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let y_plane_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            format: Some(wgpu::TextureFormat::R8Unorm),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let uv_plane_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });

        Self {
            texture,
            y_plane_view,
            uv_plane_view,
        }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn y_plane_view(&self) -> &wgpu::TextureView {
        &self.y_plane_view
    }

    pub fn uv_plane_view(&self) -> &wgpu::TextureView {
        &self.uv_plane_view
    }
}

#[derive(Clone)]
pub struct RgbaTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl RgbaTexture {
    fn new(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { texture, view }
    }
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}
