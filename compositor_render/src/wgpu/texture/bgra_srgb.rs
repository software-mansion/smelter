use crate::{wgpu::WgpuCtx, Resolution};

use super::{base::new_texture, TextureExt};

#[derive(Debug)]
pub struct BgraSrgbTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl BgraSrgbTexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let texture = new_texture(
            &ctx.device,
            None,
            wgpu::Extent3d {
                width: resolution.width as u32,
                height: resolution.height as u32,
                depth_or_array_layers: 1,
            },
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            &[wgpu::TextureFormat::Rgba8UnormSrgb],
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view }
    }

    pub fn upload(&self, ctx: &WgpuCtx, data: &[u8]) {
        self.texture.upload_data(&ctx.queue, data, 4);
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}
