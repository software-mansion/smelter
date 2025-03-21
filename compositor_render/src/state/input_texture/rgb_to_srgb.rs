use crate::{
    wgpu::{texture::RgbaLinearTexture, WgpuCtx},
    Resolution,
};

pub(super) struct RgbToSrgbConverter {
    pub texture: RgbaLinearTexture,
}

impl RgbToSrgbConverter {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self {
            texture: RgbaLinearTexture::new(ctx, resolution),
        }
    }

    pub fn convert(&self, ctx: &WgpuCtx, dest: &wgpu::Texture) {
        copy_texture_to_texture(ctx, self.texture.texture(), dest);
    }

    pub fn size(&self) -> wgpu::Extent3d {
        self.texture.size()
    }
}

pub(super) fn copy_texture_to_texture(
    ctx: &WgpuCtx,
    source: &wgpu::Texture,
    destination: &wgpu::Texture,
) {
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("copy static image asset to texture"),
        });

    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::All,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            texture: source,
        },
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::All,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            texture: destination,
        },
        source.size(),
    );

    ctx.queue.submit(Some(encoder.finish()));
}
