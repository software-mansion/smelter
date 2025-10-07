use crate::{Resolution, wgpu::WgpuCtx};

use super::base::{TextureExt, new_texture};

#[derive(Debug)]
pub struct RgbaSrgbTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl RgbaSrgbTexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self::new_texture(&ctx.device, resolution)
    }

    pub fn empty(device: &wgpu::Device) -> Self {
        Self::new_texture(device, Resolution::ONE_PIXEL)
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
            &[wgpu::TextureFormat::Rgba8UnormSrgb],
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });
        Self { texture, view }
    }

    pub fn new_bind_group(&self, ctx: &WgpuCtx) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
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

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn texture_owned(self) -> wgpu::Texture {
        self.texture
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}
