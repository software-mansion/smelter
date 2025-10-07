use crate::{Resolution, wgpu::WgpuCtx};

use super::base::{TextureExt, new_texture};

#[derive(Debug)]
pub struct RgbaMultiViewTexture {
    texture: wgpu::Texture,
    linear_view: wgpu::TextureView,
    srgb_view: wgpu::TextureView,
}

impl RgbaMultiViewTexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self::new_texture(&ctx.device, resolution)
    }

    fn new_texture(device: &wgpu::Device, resolution: Resolution) -> Self {
        let size = wgpu::Extent3d {
            width: resolution.width as u32,
            height: resolution.height as u32,
            depth_or_array_layers: 1,
        };
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING;

        let texture = new_texture(
            device,
            None,
            size,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            usage,
            &[
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::TextureFormat::Rgba8Unorm,
            ],
        );

        let linear_view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let srgb_view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });

        Self {
            texture,
            linear_view,
            srgb_view,
        }
    }

    pub fn new_srgb_bind_group(&self, ctx: &WgpuCtx) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
            layout: &ctx.format.single_texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.srgb_view),
            }],
        })
    }

    pub fn new_linear_bind_group(&self, ctx: &WgpuCtx) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
            layout: &ctx.format.single_texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.linear_view),
            }],
        })
    }

    pub fn upload(&self, ctx: &WgpuCtx, data: &[u8]) {
        self.texture.upload_data(&ctx.queue, data, 4);
    }

    pub fn size(&self) -> wgpu::Extent3d {
        self.texture.size()
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn texture_owned(self) -> wgpu::Texture {
        self.texture
    }

    pub fn linear_view(&self) -> &wgpu::TextureView {
        &self.linear_view
    }

    pub fn srgb_view(&self) -> &wgpu::TextureView {
        &self.srgb_view
    }
}
