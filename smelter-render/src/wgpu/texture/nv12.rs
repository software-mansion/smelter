use std::sync::Arc;

use crate::{
    NvPlanes, Resolution,
    wgpu::{
        WgpuCtx,
        texture::{TextureExt, base::new_texture},
    },
};

#[derive(Debug)]
pub struct NV12Texture {
    texture: Arc<wgpu::Texture>,
    secondary_texture: Option<Arc<wgpu::Texture>>,
    view_y: wgpu::TextureView,
    view_uv: wgpu::TextureView,
}

#[derive(Debug, thiserror::Error)]
#[error("Passed invalid texture. Expected: {expected}, Actual: {actual}")]
pub struct NV12TextureViewCreateError {
    expected: String,
    actual: String,
}

impl NV12Texture {
    pub fn from_wgpu_texture(
        texture: Arc<wgpu::Texture>,
    ) -> Result<Self, NV12TextureViewCreateError> {
        let expected = (wgpu::TextureDimension::D2, wgpu::TextureFormat::NV12);
        let actual = (texture.dimension(), texture.format());

        if expected != actual {
            return Err(NV12TextureViewCreateError {
                expected: format!("{expected:?}"),
                actual: format!("{actual:?}"),
            });
        }

        let view_y = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("y plane nv12 texture view"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            format: Some(wgpu::TextureFormat::R8Unorm),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });

        let view_uv = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("uv plane nv12 texture view"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });

        Ok(Self {
            texture,
            secondary_texture: None,
            view_y,
            view_uv,
        })
    }

    pub fn new_uploadable(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING;

        let texture = new_texture(
            &ctx.device,
            Some("nv12 y plane texture"),
            wgpu::Extent3d {
                width: resolution.width as u32,
                height: resolution.height as u32,
                ..Default::default()
            },
            wgpu::TextureFormat::R8Unorm,
            usage,
            &[],
        );

        let view_y = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let secondary_texture = new_texture(
            &ctx.device,
            Some("nv12 uv plane texture"),
            wgpu::Extent3d {
                width: resolution.width as u32 / 2,
                height: resolution.height as u32 / 2,
                ..Default::default()
            },
            wgpu::TextureFormat::Rg8Unorm,
            usage,
            &[],
        );

        let view_uv = secondary_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture: texture.into(),
            secondary_texture: Some(secondary_texture.into()),
            view_y,
            view_uv,
        }
    }

    pub fn uploadable(&self) -> bool {
        self.secondary_texture.is_some()
    }

    pub fn upload(&self, ctx: &WgpuCtx, planes: &NvPlanes) {
        self.texture.upload_data(&ctx.queue, &planes.y_plane, 1);
        self.secondary_texture
            .as_ref()
            .unwrap()
            .upload_data(&ctx.queue, &planes.uv_planes, 2);
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn new_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                },
            ],
        })
    }

    pub fn new_bind_group(&self, ctx: &WgpuCtx) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nv12 texture bind group"),
            layout: &ctx.format.nv12_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.view_y),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.view_uv),
                },
            ],
        })
    }
}
