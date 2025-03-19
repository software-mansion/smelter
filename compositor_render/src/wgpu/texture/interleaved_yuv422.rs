use crate::{wgpu::WgpuCtx, Resolution};

use super::{base::new_texture, TextureExt};

#[derive(Debug)]
pub struct InterleavedYuv422Texture {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(crate) resolution: Resolution,
}

impl InterleavedYuv422Texture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let texture = new_texture(
            &ctx.device,
            None,
            wgpu::Extent3d {
                width: resolution.width as u32 / 2,
                height: resolution.height as u32,
                depth_or_array_layers: 1,
            },
            // r - u
            // g - y1
            // b - v
            // a - y2
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            &[wgpu::TextureFormat::Rgba8Unorm],
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            resolution,
            texture,
            view,
        }
    }

    pub fn new_bind_group(&self, ctx: &WgpuCtx) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Interleaved YUV 4:2:2 texture bind group"),
            layout: &ctx.format.single_texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.view),
            }],
        })
    }

    pub fn upload(&self, ctx: &WgpuCtx, data: &[u8]) {
        self.texture.upload_data(&ctx.queue, data, 4);
    }
}
