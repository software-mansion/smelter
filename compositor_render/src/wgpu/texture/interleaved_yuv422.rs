use crate::{
    wgpu::{common_pipeline, WgpuCtx},
    Resolution,
};

use super::base::Texture;

#[derive(Debug)]
pub struct InterleavedYuv422Texture {
    pub(super) texture: Texture,
    pub(super) resolution: Resolution,
}

impl InterleavedYuv422Texture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self {
            resolution,
            texture: Texture::new(
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
            ),
        }
    }

    pub fn new_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        common_pipeline::create_single_texture_bgl(device)
    }

    pub(super) fn new_bind_group(
        &self,
        ctx: &WgpuCtx,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Interleaved YUV 4:2:2 texture bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.texture.view),
            }],
        })
    }

    pub fn upload(&self, ctx: &WgpuCtx, data: &[u8]) {
        self.texture.upload_data(&ctx.queue, data, 4);
    }
}
